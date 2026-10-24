#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::ProxySettings;

#[cfg(not(target_os = "macos"))]
mod portable {
    use anyhow::Result;
    use serde::{Deserialize, Serialize};
    use std::net::SocketAddr;
    use sysproxy::Sysproxy;

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
    pub(crate) struct ProxySettings {
        enable: bool,
        host: String,
        port: u16,
        bypass: String,
    }
    impl ProxySettings {
        pub(crate) fn read() -> Result<Self> {
            let p = Sysproxy::get_system_proxy()?;
            Ok(Self {
                enable: p.enable,
                host: p.host,
                port: p.port,
                bypass: p.bypass,
            })
        }
        pub(crate) fn read_current(&self) -> Result<Self> {
            Self::read()
        }
        pub(crate) fn for_listener(&self, addr: SocketAddr) -> Result<Self> {
            Ok(Self {
                enable: true,
                host: if addr.ip().is_unspecified() {
                    "127.0.0.1".into()
                } else {
                    addr.ip().to_string()
                },
                port: addr.port(),
                bypass: "localhost,127.0.0.1/8".into(),
            })
        }
        pub(crate) fn write(&self, _expected: &Self) -> Result<()> {
            Sysproxy {
                enable: self.enable,
                host: self.host.clone(),
                port: self.port,
                bypass: self.bypass.clone(),
            }
            .set_system_proxy()?;
            Ok(())
        }
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) use portable::ProxySettings;
