//! Sound & Audio: the form beside the "Sound & Audio" row. Volume and mute
//! of the speakers and the microphone.

use gpui::App;
use services::AudioCommand;
use ui::IconName;

use super::settings_form::{self as rows, Number};
use crate::launcher::content::{Detail, Form, Item};
use crate::state::AppState;

pub fn item() -> Item {
    Item::new("sound", "Sound & Audio")
        .subtitle("Output volume, microphone, and audio devices")
        .icon(IconName::Volume)
        .keywords("sound audio volume speakers headphones mic microphone mute")
        .actions(super::system::edit())
        .detail(Detail::new().form(sound_form))
}

fn sound_form(form: &mut Form, cx: &App) {
    let service = AppState::audio(cx);
    let audio = service.get();

    rows::title(
        form,
        "Sound & Audio",
        "Volume and mute of the default speakers and microphone",
    );

    rows::heading(form, "Output");
    rows::number(
        form,
        Number {
            id: "sound-out".into(),
            label: "Volume",
            description: "Master volume of the speakers or headphones",
            value: f64::from(audio.sink_volume),
            min: 0.0,
            max: 100.0,
            step: 5.0,
            unit: Some("%"),
            default: 50.0,
        },
        {
            let service = service.clone();
            move |v, _| service.dispatch(AudioCommand::SetSinkVolume(v.round() as u8))
        },
        cx,
    );
    rows::toggle(
        form,
        "sound-out-mute",
        "Muted",
        "Silence the speakers",
        audio.sink_muted,
        {
            let service = service.clone();
            move |_, _| service.dispatch(AudioCommand::ToggleSinkMute)
        },
        cx,
    );

    rows::heading(form, "Input");
    rows::number(
        form,
        Number {
            id: "sound-in".into(),
            label: "Volume",
            description: "Gain of the microphone",
            value: f64::from(audio.source_volume),
            min: 0.0,
            max: 100.0,
            step: 5.0,
            unit: Some("%"),
            default: 50.0,
        },
        {
            let service = service.clone();
            move |v, _| service.dispatch(AudioCommand::SetSourceVolume(v.round() as u8))
        },
        cx,
    );
    rows::toggle(
        form,
        "sound-in-mute",
        "Muted",
        "Silence the microphone",
        audio.source_muted,
        {
            let service = service.clone();
            move |_, _| service.dispatch(AudioCommand::ToggleSourceMute)
        },
        cx,
    );
}
