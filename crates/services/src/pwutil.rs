//! What the services that run a PipeWire filter-chain (the equalizer, the
//! microphone) have in common: the child process, the config's quoting, the
//! `Props` pod that sets controls live and the `default` metadata.

use std::io::Cursor;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::Context as _;
use pipewire::{
    metadata::Metadata,
    spa::{
        param::ParamType,
        pod::{Object, Property, Value, serialize::PodSerializer},
    },
};

/// `SPA_PROP_params`: the property of a `Props` object that carries
/// `"node:control" value` pairs.
const SPA_PROP_PARAMS: u32 = 0x8_0001;
const SPA_TYPE_OBJECT_PROPS: u32 = 0x4_0002;

/// `text` as a string of the config's JSON-ish syntax.
pub fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `$XDG_RUNTIME_DIR/conchy/<name>`.
pub fn runtime_file(name: &str) -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("conchy").join(name)
}

/// A `Props` pod setting `controls` (`"node:control"` and its value).
pub fn params_pod(controls: &[(String, f32)]) -> Option<Vec<u8>> {
    let items = controls
        .iter()
        .flat_map(|(name, value)| [Value::String(name.clone()), Value::Float(*value)])
        .collect();
    let object = Value::Object(Object {
        type_: SPA_TYPE_OBJECT_PROPS,
        id: ParamType::Props.as_raw(),
        properties: vec![Property::new(SPA_PROP_PARAMS, Value::Struct(items))],
    });
    PodSerializer::serialize(Cursor::new(Vec::new()), &object)
        .ok()
        .map(|(cursor, _)| cursor.into_inner())
}

/// `{"name":"alsa_output..."}` from a `default` metadata value.
pub fn default_name(value: Option<&str>) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(value?).ok()?;
    json.get("name")?.as_str().map(str::to_string)
}

/// Make `node` the default for `key` (`default.configured.audio.sink`...).
pub fn set_default(metadata: &Metadata, key: &str, node: &str) {
    let value = format!("{{\"name\":{}}}", quoted(node));
    metadata.set_property(0, key, Some("Spa:String:JSON"), Some(&value));
}

/// Start `pipewire -c <conf>` with its output in `log`. The child dies with
/// the shell (`PDEATHSIG`).
pub fn spawn_chain(conf: &Path, log: &Path) -> anyhow::Result<Child> {
    let log_file = std::fs::File::create(log).ok();
    let mut command = Command::new("pipewire");
    command
        .arg("-c")
        .arg(conf)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_file.map_or_else(Stdio::null, Stdio::from));
    // SAFETY: only `prctl` runs between fork and exec, which is
    // async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .spawn()
        .context("starting `pipewire -c` for the filter-chain")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_escapes_quotes_and_backslashes() {
        assert_eq!(quoted("a\"b\\c"), r#""a\"b\\c""#);
    }

    #[test]
    fn the_default_name_is_read_from_the_json() {
        assert_eq!(
            default_name(Some(r#"{"name":"alsa_output.x"}"#)).as_deref(),
            Some("alsa_output.x")
        );
        assert_eq!(default_name(Some("not json")), None);
        assert_eq!(default_name(None), None);
    }

    #[test]
    fn the_props_pod_is_built() {
        let bytes = params_pod(&[("band_9:Gain".to_string(), 1.0)]).unwrap();
        assert!(pipewire::spa::pod::Pod::from_bytes(&bytes).is_some());
        assert!(String::from_utf8_lossy(&bytes).contains("band_9:Gain"));
    }
}
