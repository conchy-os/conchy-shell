use std::collections::HashMap;

use zbus::{Result, proxy, zvariant::OwnedValue};

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
pub trait MprisPlayer {
    fn next(&self) -> Result<()>;
    fn play_pause(&self) -> Result<()>;
    fn previous(&self) -> Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> Result<String>;
    #[zbus(property)]
    fn metadata(&self) -> Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn set_volume(&self, v: f64) -> Result<()>;
    #[zbus(property)]
    fn volume(&self) -> Result<f64>;
    #[zbus(property)]
    fn can_control(&self) -> Result<bool>;
    /// Never announced through `PropertiesChanged`, so it must not be cached.
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> Result<i64>;

    #[zbus(signal)]
    fn seeked(&self, position: i64) -> Result<()>;
}

#[proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2"
)]
pub trait MprisRoot {
    #[zbus(property)]
    fn identity(&self) -> Result<String>;
}
