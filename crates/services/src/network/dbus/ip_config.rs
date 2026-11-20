//! NetworkManager IP4Config and IP6Config D-Bus proxies.

use std::collections::HashMap;
use zbus::proxy;
use zbus::zvariant::OwnedValue;

#[proxy(
    interface = "org.freedesktop.NetworkManager.IP4Config",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait Ip4Config {
    /// Gateway property.
    #[zbus(property)]
    fn gateway(&self) -> zbus::Result<String>;

    /// AddressData property: array of dicts with "address" and "prefix".
    #[zbus(property)]
    fn address_data(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;

    /// NameserverData property: array of dicts with "address".
    #[zbus(property)]
    fn nameserver_data(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.IP6Config",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait Ip6Config {
    /// Gateway property.
    #[zbus(property)]
    fn gateway(&self) -> zbus::Result<String>;

    /// AddressData property: array of dicts with "address" and "prefix".
    #[zbus(property)]
    fn address_data(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
}
