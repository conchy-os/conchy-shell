//! The filter graph of the microphone: which stages the voice goes through,
//! what each is set to, and the `pipewire -c` file that builds it.
//!
//! The order is the one EasyEffects uses for a voice: rumble filter, noise
//! remover, gate, equalizer, compressor, limiter, and a last gain. A stage
//! that needs a plugin the machine does not have is left out (and the UI
//! says which package would bring it back); the built-in ones (rumble
//! filter, equalizer, gain) always work.

use std::path::PathBuf;

use super::model::{Denoiser, Param, Voice};
use crate::pwutil::quoted;

/// The virtual microphone every app is pointed at while the chain is on.
pub const SOURCE_NAME: &str = "conchy_mic_source";
const CAPTURE_NAME: &str = "conchy_mic_in";

const LSP_GATE: &str = "http://lsp-plug.in/plugins/lv2/gate_mono";
const LSP_COMPRESSOR: &str = "http://lsp-plug.in/plugins/lv2/compressor_mono";
const LSP_LIMITER: &str = "http://lsp-plug.in/plugins/lv2/limiter_mono";

/// The packages the optional stages come from, as the distribution names
/// them.
pub const PACKAGE_LSP: &str = "lsp-plugins-lv2";
pub const PACKAGE_RNNOISE: &str = "noise-suppression-for-voice";
pub const PACKAGE_DEEPFILTER: &str = "deepfilternet-ladspa";

/// Which of the plugins the stages are made of are installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Plugins {
    /// LSP: gate, compressor and limiter.
    pub lsp: bool,
    pub rnnoise: bool,
    pub deepfilter: bool,
}

impl Plugins {
    pub fn detect() -> Self {
        Self::detect_in(&lv2_dirs(), &ladspa_dirs())
    }

    fn detect_in(lv2: &[PathBuf], ladspa: &[PathBuf]) -> Self {
        let lsp = lv2.iter().any(|dir| {
            ["gate_mono", "compressor_mono", "limiter_mono"]
                .iter()
                .all(|name| dir.join("lsp-plugins.lv2").join(format!("{name}.ttl")).is_file())
        });
        let has = |file: &str| ladspa.iter().any(|dir| dir.join(file).is_file());
        Self {
            lsp,
            rnnoise: has("librnnoise_ladspa.so"),
            deepfilter: has("libdeep_filter_ladspa.so"),
        }
    }

    /// The packages the voice asks for that are not installed.
    pub fn missing(&self, voice: &Voice) -> Vec<&'static str> {
        let mut missing = Vec::new();
        match voice.denoiser() {
            Denoiser::RnNoise if !self.rnnoise => missing.push(PACKAGE_RNNOISE),
            Denoiser::DeepFilter if !self.deepfilter => missing.push(PACKAGE_DEEPFILTER),
            _ => {}
        }
        let dynamics = [Param::GateOn, Param::CompOn, Param::LimiterOn];
        if !self.lsp && dynamics.iter().any(|p| voice.on(*p)) {
            missing.push(PACKAGE_LSP);
        }
        missing
    }
}

fn search_path(variable: &str, defaults: &[&str], home_relative: &str) -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os(variable) {
        let dirs: Vec<PathBuf> = std::env::split_paths(&value)
            .filter(|dir| !dir.as_os_str().is_empty())
            .collect();
        if !dirs.is_empty() {
            return dirs;
        }
    }
    let mut dirs: Vec<PathBuf> = crate::fsutil::home()
        .map(|home| home.join(home_relative))
        .into_iter()
        .collect();
    dirs.extend(defaults.iter().map(PathBuf::from));
    dirs
}

fn lv2_dirs() -> Vec<PathBuf> {
    search_path(
        "LV2_PATH",
        &["/usr/lib/lv2", "/usr/lib64/lv2", "/usr/local/lib/lv2"],
        ".lv2",
    )
}

fn ladspa_dirs() -> Vec<PathBuf> {
    search_path(
        "LADSPA_PATH",
        &["/usr/lib/ladspa", "/usr/lib64/ladspa", "/usr/local/lib/ladspa"],
        ".ladspa",
    )
}

/// One step of the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Highpass,
    RnNoise,
    DeepFilter,
    Gate,
    Equalizer,
    Compressor,
    Limiter,
    Gain,
}

/// The stages the voice goes through, first to last, given what is
/// installed.
pub fn stages(voice: &Voice, plugins: &Plugins) -> Vec<Stage> {
    let mut stages = Vec::new();
    if voice.on(Param::HighpassOn) {
        stages.push(Stage::Highpass);
    }
    match voice.denoiser() {
        Denoiser::RnNoise if plugins.rnnoise => stages.push(Stage::RnNoise),
        Denoiser::DeepFilter if plugins.deepfilter => stages.push(Stage::DeepFilter),
        _ => {}
    }
    if plugins.lsp && voice.on(Param::GateOn) {
        stages.push(Stage::Gate);
    }
    if voice.on(Param::EqOn) {
        stages.push(Stage::Equalizer);
    }
    if plugins.lsp && voice.on(Param::CompOn) {
        stages.push(Stage::Compressor);
    }
    if plugins.lsp && voice.on(Param::LimiterOn) {
        stages.push(Stage::Limiter);
    }
    stages.push(Stage::Gain);
    stages
}

/// The graph nodes of a stage, in order, and where the signal goes in and
/// comes out.
struct Piece {
    /// `{ type = ... }` entries.
    nodes: Vec<String>,
    /// Node names, in signal order, with their input and output port.
    path: Vec<(&'static str, &'static str, &'static str)>,
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn builtin(name: &str, label: &str, controls: &str) -> String {
    format!("{{ type = builtin name = {name} label = {label} control = {{ {controls} }} }}")
}

fn lv2(name: &str, plugin: &str, controls: &str) -> String {
    format!("{{ type = lv2 name = {name} plugin = {} control = {{ {controls} }} }}", quoted(plugin))
}

fn piece(stage: Stage, voice: &Voice) -> Piece {
    let get = |p| voice.get(p);
    match stage {
        Stage::Highpass => Piece {
            nodes: vec![builtin(
                "hpf",
                "bq_highpass",
                &format!("Freq = {:.1} Q = 0.707 Gain = 0.0", get(Param::HighpassHz)),
            )],
            path: vec![("hpf", "In", "Out")],
        },
        Stage::RnNoise => Piece {
            nodes: vec![format!(
                "{{ type = ladspa name = rnn plugin = librnnoise_ladspa label = noise_suppressor_mono control = {{ \"VAD Threshold (%)\" = {:.1} \"VAD Grace Period (ms)\" = 200.0 \"Retroactive VAD Grace (ms)\" = 0.0 }} }}",
                get(Param::RnnoiseVad)
            )],
            path: vec![("rnn", "Input", "Output")],
        },
        Stage::DeepFilter => Piece {
            nodes: vec![format!(
                "{{ type = ladspa name = dfn plugin = libdeep_filter_ladspa label = deep_filter_mono control = {{ \"Attenuation Limit (dB)\" = {:.1} }} }}",
                get(Param::DeepFilterLimit)
            )],
            path: vec![("dfn", "Audio In", "Audio Out")],
        },
        Stage::Gate => Piece {
            nodes: vec![lv2(
                "gate",
                LSP_GATE,
                &format!(
                    "gt = {:.6} gr = {:.6} at = 10.0 rt = 150.0 hold = 20.0",
                    db_to_linear(get(Param::GateThreshold)),
                    db_to_linear(get(Param::GateReduction)),
                ),
            )],
            path: vec![("gate", "in", "out")],
        },
        Stage::Equalizer => Piece {
            nodes: vec![
                builtin("eq_low", "bq_lowshelf", &format!("Freq = 120.0 Q = 0.7 Gain = {:.1}", get(Param::EqLow))),
                builtin("eq_mud", "bq_peaking", &format!("Freq = 300.0 Q = 1.0 Gain = {:.1}", get(Param::EqMud))),
                builtin("eq_presence", "bq_peaking", &format!("Freq = 3500.0 Q = 0.9 Gain = {:.1}", get(Param::EqPresence))),
                builtin("eq_air", "bq_highshelf", &format!("Freq = 10000.0 Q = 0.7 Gain = {:.1}", get(Param::EqAir))),
            ],
            path: vec![
                ("eq_low", "In", "Out"),
                ("eq_mud", "In", "Out"),
                ("eq_presence", "In", "Out"),
                ("eq_air", "In", "Out"),
            ],
        },
        Stage::Compressor => Piece {
            nodes: vec![lv2(
                "comp",
                LSP_COMPRESSOR,
                &format!(
                    "al = {:.6} cr = {:.2} at = 15.0 rt = 120.0 mk = {:.6}",
                    db_to_linear(get(Param::CompThreshold)),
                    get(Param::CompRatio),
                    db_to_linear(get(Param::CompMakeup)),
                ),
            )],
            path: vec![("comp", "in", "out")],
        },
        Stage::Limiter => Piece {
            nodes: vec![lv2(
                "lim",
                LSP_LIMITER,
                &format!("th = {:.6} boost = 0", db_to_linear(get(Param::LimiterCeiling))),
            )],
            path: vec![("lim", "in", "out")],
        },
        Stage::Gain => Piece {
            nodes: vec![builtin(
                "out",
                "linear",
                &format!("Mult = {:.6} Add = 0.0", db_to_linear(get(Param::OutputGain))),
            )],
            path: vec![("out", "In", "Out")],
        },
    }
}

/// The live controls of the chain, as the pairs a `Props` update carries.
/// Only the stages that are in the graph have any.
pub fn controls(voice: &Voice, plugins: &Plugins) -> Vec<(String, f32)> {
    let get = |p| voice.get(p);
    let mut controls = Vec::new();
    let mut push = |name: &str, value: f32| controls.push((name.to_string(), value));
    for stage in stages(voice, plugins) {
        match stage {
            Stage::Highpass => push("hpf:Freq", get(Param::HighpassHz)),
            Stage::RnNoise => push("rnn:VAD Threshold (%)", get(Param::RnnoiseVad)),
            Stage::DeepFilter => push("dfn:Attenuation Limit (dB)", get(Param::DeepFilterLimit)),
            Stage::Gate => {
                push("gate:gt", db_to_linear(get(Param::GateThreshold)));
                push("gate:gr", db_to_linear(get(Param::GateReduction)));
            }
            Stage::Equalizer => {
                push("eq_low:Gain", get(Param::EqLow));
                push("eq_mud:Gain", get(Param::EqMud));
                push("eq_presence:Gain", get(Param::EqPresence));
                push("eq_air:Gain", get(Param::EqAir));
            }
            Stage::Compressor => {
                push("comp:al", db_to_linear(get(Param::CompThreshold)));
                push("comp:cr", get(Param::CompRatio));
                push("comp:mk", db_to_linear(get(Param::CompMakeup)));
            }
            Stage::Limiter => push("lim:th", db_to_linear(get(Param::LimiterCeiling))),
            Stage::Gain => push("out:Mult", db_to_linear(get(Param::OutputGain))),
        }
    }
    controls
}

/// The `pipewire -c` file for a chain that takes the voice from `input`,
/// with the controls at the values of `voice`.
pub fn chain_config(input: &str, voice: &Voice, plugins: &Plugins) -> String {
    let mut nodes = String::new();
    let mut links = String::new();
    let mut path: Vec<(&str, &str, &str)> = Vec::new();
    for stage in stages(voice, plugins) {
        let piece = piece(stage, voice);
        for node in piece.nodes {
            nodes.push_str(&format!("          {node}\n"));
        }
        path.extend(piece.path);
    }
    for pair in path.windows(2) {
        let (from, _, out) = pair[0];
        let (to, input_port, _) = pair[1];
        links.push_str(&format!(
            "          {{ output = {} input = {} }}\n",
            quoted(&format!("{from}:{out}")),
            quoted(&format!("{to}:{input_port}")),
        ));
    }
    let (first, first_in, _) = path[0];
    let (last, _, last_out) = path[path.len() - 1];

    format!(
        r#"# Generated by the shell's microphone settings; rewritten every time the chain starts.
context.properties = {{
    log.level = 1
}}
context.spa-libs = {{
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}}
context.modules = [
    {{ name = libpipewire-module-rt args = {{ }} flags = [ ifexists nofail ] }}
    {{ name = libpipewire-module-protocol-native }}
    {{ name = libpipewire-module-client-node }}
    {{ name = libpipewire-module-adapter }}
    {{ name = libpipewire-module-filter-chain
        args = {{
            node.description = "Conchy Microphone"
            media.name = "Conchy Microphone"
            filter.graph = {{
                nodes = [
{nodes}                ]
                links = [
{links}                ]
                inputs = [ {inputs} ]
                outputs = [ {outputs} ]
            }}
            audio.rate = 48000
            audio.channels = 1
            audio.position = [ MONO ]
            capture.props = {{
                node.name = "{CAPTURE_NAME}"
                node.passive = true
                target.object = {target}
            }}
            playback.props = {{
                node.name = "{SOURCE_NAME}"
                node.description = "Conchy Microphone"
                media.class = Audio/Source
            }}
        }}
    }}
]
"#,
        inputs = quoted(&format!("{first}:{first_in}")),
        outputs = quoted(&format!("{last}:{last_out}")),
        target = quoted(input),
    )
}

/// The config file of the chain, next to the other runtime files.
pub fn conf_path() -> PathBuf {
    crate::pwutil::runtime_file("mic-chain.conf")
}

pub fn log_path() -> PathBuf {
    crate::pwutil::runtime_file("mic-chain.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Plugins = Plugins {
        lsp: true,
        rnnoise: true,
        deepfilter: true,
    };
    const NONE: Plugins = Plugins {
        lsp: false,
        rnnoise: false,
        deepfilter: false,
    };

    fn studio() -> Voice {
        let mut settings = super::super::model::Settings::default();
        settings.apply_preset("Streaming");
        settings.voice
    }

    #[test]
    fn a_clean_voice_is_a_rumble_filter_and_a_gain() {
        let voice = Voice::default();
        assert_eq!(stages(&voice, &ALL), vec![Stage::Highpass, Stage::Gain]);
        let text = chain_config("alsa_input.x", &voice, &ALL);
        assert!(text.contains("label = bq_highpass"));
        assert!(text.contains("output = \"hpf:Out\" input = \"out:In\""));
        assert!(text.contains("inputs = [ \"hpf:In\" ]"));
        assert!(text.contains("outputs = [ \"out:Out\" ]"));
        assert!(text.contains("media.class = Audio/Source"));
        assert!(text.contains("target.object = \"alsa_input.x\""));
    }

    #[test]
    fn a_voice_with_no_stage_at_all_is_a_lone_gain() {
        let mut voice = Voice::default();
        voice.set(Param::HighpassOn, 0.0);
        let text = chain_config("m", &voice, &NONE);
        assert!(text.contains("inputs = [ \"out:In\" ]"));
        assert!(text.contains("outputs = [ \"out:Out\" ]"));
        assert!(!text.contains("links = [\n          {"));
    }

    #[test]
    fn stages_run_in_the_voice_order() {
        let voice = studio();
        assert_eq!(
            stages(&voice, &ALL),
            vec![
                Stage::Highpass,
                Stage::RnNoise,
                Stage::Gate,
                Stage::Equalizer,
                Stage::Compressor,
                Stage::Limiter,
                Stage::Gain,
            ]
        );
        let text = chain_config("m", &voice, &ALL);
        assert!(text.contains("output = \"hpf:Out\" input = \"rnn:Input\""));
        assert!(text.contains("output = \"rnn:Output\" input = \"gate:in\""));
        assert!(text.contains("output = \"gate:out\" input = \"eq_low:In\""));
        assert!(text.contains("output = \"eq_air:Out\" input = \"comp:in\""));
        assert!(text.contains("output = \"lim:out\" input = \"out:In\""));
        assert!(text.contains("plugin = \"http://lsp-plug.in/plugins/lv2/gate_mono\""));
    }

    #[test]
    fn stages_of_missing_plugins_are_left_out_and_reported() {
        let voice = studio();
        assert_eq!(
            stages(&voice, &NONE),
            vec![Stage::Highpass, Stage::Equalizer, Stage::Gain]
        );
        assert_eq!(
            NONE.missing(&voice),
            vec![PACKAGE_RNNOISE, PACKAGE_LSP]
        );
        assert!(ALL.missing(&voice).is_empty());
        assert!(NONE.missing(&Voice::default()).is_empty());
        // The other noise remover brings its own package.
        let mut deep = voice;
        deep.set(Param::Denoiser, 2.0);
        assert_eq!(
            Plugins { deepfilter: false, ..ALL }.missing(&deep),
            vec![PACKAGE_DEEPFILTER]
        );
    }

    #[test]
    fn every_control_names_a_node_of_the_graph() {
        let voice = studio();
        let text = chain_config("m", &voice, &ALL);
        for (name, _) in controls(&voice, &ALL) {
            let node = name.split(':').next().unwrap();
            assert!(text.contains(&format!("name = {node} ")), "{name}");
        }
    }

    #[test]
    fn controls_are_converted_from_db() {
        let mut voice = Voice::default();
        voice.set(Param::OutputGain, -6.0);
        voice.set(Param::HighpassHz, 120.0);
        let controls = controls(&voice, &NONE);
        assert_eq!(controls[0], ("hpf:Freq".to_string(), 120.0));
        assert_eq!(controls[1].0, "out:Mult");
        assert!((controls[1].1 - 0.501).abs() < 0.01);
    }

    #[test]
    fn the_deepfilter_ports_are_quoted_for_their_spaces() {
        let mut voice = Voice::default();
        voice.set(Param::Denoiser, 2.0);
        let text = chain_config("m", &voice, &ALL);
        assert!(text.contains("output = \"dfn:Audio Out\" input = \"out:In\""));
        assert!(text.contains("\"Attenuation Limit (dB)\" = 70.0"));
    }

    #[test]
    fn an_odd_device_name_cannot_break_out_of_the_config() {
        let text = chain_config("a\"b\\c", &Voice::default(), &NONE);
        assert!(text.contains(r#"target.object = "a\"b\\c""#));
    }

    #[test]
    fn plugins_are_found_where_they_are_installed() {
        let root = std::env::temp_dir().join(format!("conchy-mic-plugins-{}", std::process::id()));
        let lv2 = root.join("lv2");
        let ladspa = root.join("ladspa");
        std::fs::create_dir_all(lv2.join("lsp-plugins.lv2")).unwrap();
        std::fs::create_dir_all(&ladspa).unwrap();
        assert_eq!(Plugins::detect_in(&[lv2.clone()], &[ladspa.clone()]), NONE);

        for name in ["gate_mono", "compressor_mono", "limiter_mono"] {
            std::fs::write(lv2.join("lsp-plugins.lv2").join(format!("{name}.ttl")), "").unwrap();
        }
        std::fs::write(ladspa.join("librnnoise_ladspa.so"), "").unwrap();
        let found = Plugins::detect_in(&[lv2], &[ladspa]);
        assert_eq!(
            found,
            Plugins {
                lsp: true,
                rnnoise: true,
                deepfilter: false
            }
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
