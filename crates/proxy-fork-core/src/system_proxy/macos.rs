use anyhow::{Context, Result, ensure};
use core_foundation::{
    base::TCFType,
    data::CFData,
    propertylist::{self, CFPropertyList},
    string::CFString,
};
use security_framework_sys::authorization as auth;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, ptr};
use system_configuration::{
    dynamic_store::SCDynamicStoreBuilder, preferences::SCPreferences, sys::preferences::*,
};

unsafe extern "C" {
    fn SCPreferencesPathGetValue(
        prefs: SCPreferencesRef,
        path: core_foundation::string::CFStringRef,
    ) -> propertylist::CFPropertyListRef;
    fn SCPreferencesPathSetValue(
        prefs: SCPreferencesRef,
        path: core_foundation::string::CFStringRef,
        value: propertylist::CFPropertyListRef,
    ) -> u8;
    fn SCPreferencesPathRemoveValue(
        prefs: SCPreferencesRef,
        path: core_foundation::string::CFStringRef,
    ) -> u8;
    fn SCError() -> i32;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ProxySettings {
    service_id: String,
    proxies_xml: Option<String>,
}
impl PartialEq for ProxySettings {
    fn eq(&self, other: &Self) -> bool {
        self.service_id == other.service_id
            && matches!((self.value(), other.value()), (Ok(a), Ok(b)) if a == b)
    }
}
impl ProxySettings {
    fn value(&self) -> Result<Option<plist::Value>> {
        self.proxies_xml
            .as_ref()
            .map(|xml| plist::Value::from_reader(std::io::Cursor::new(xml)).map_err(Into::into))
            .transpose()
    }
    fn path(&self) -> Result<CFString> {
        ensure!(
            self.service_id.len() == 36
                && self
                    .service_id
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() || c == b'-'),
            "invalid network service identity"
        );
        Ok(CFString::new(&format!(
            "/NetworkServices/{}/Proxies",
            self.service_id
        )))
    }
    pub(crate) fn read() -> Result<Self> {
        let store = SCDynamicStoreBuilder::new("Proxy Fork").build();
        let primary = store
            .get("State:/Network/Global/IPv4")
            .or_else(|| store.get("State:/Network/Global/IPv6"))
            .context("no primary network service")?;
        let primary = plist_value(primary.as_concrete_TypeRef())?;
        let service_id = primary
            .as_dictionary()
            .and_then(|d| d.get("PrimaryService"))
            .and_then(plist::Value::as_string)
            .context("no primary network service identity")?;
        Self::read_for(&open_preferences(None)?, service_id)
    }
    fn read_for(prefs: &SCPreferences, service_id: &str) -> Result<Self> {
        let mut snapshot = Self {
            service_id: service_id.into(),
            proxies_xml: None,
        };
        let path = snapshot.path()?;
        let service = CFString::new(&format!("/NetworkServices/{service_id}"));
        // All pointers are borrowed only while the owning preferences/string objects are alive.
        unsafe {
            ensure!(
                !SCPreferencesPathGetValue(
                    prefs.as_concrete_TypeRef(),
                    service.as_concrete_TypeRef()
                )
                .is_null(),
                "recorded network service no longer exists"
            );
            let value =
                SCPreferencesPathGetValue(prefs.as_concrete_TypeRef(), path.as_concrete_TypeRef());
            if !value.is_null() {
                let mut xml = Vec::new();
                let value = plist_value(value)?;
                ensure!(
                    value.as_dictionary().is_some(),
                    "invalid network proxy dictionary"
                );
                value.to_writer_xml(&mut xml)?;
                snapshot.proxies_xml = Some(String::from_utf8(xml)?);
            }
        }
        Ok(snapshot)
    }
    pub(crate) fn read_current(&self) -> Result<Self> {
        Self::read_for(&open_preferences(None)?, &self.service_id)
    }
    pub(crate) fn for_listener(&self, addr: SocketAddr) -> Result<Self> {
        let mut proxies = self
            .value()?
            .and_then(plist::Value::into_dictionary)
            .unwrap_or_default();
        ensure!(
            !proxies.keys().any(|key| {
                let key = key.to_ascii_lowercase();
                key.contains("password") || key.contains("user") || key.contains("auth")
            }),
            "authenticated system proxy settings cannot be safely taken over"
        );
        let host = if addr.ip().is_unspecified() {
            "127.0.0.1".into()
        } else {
            addr.ip().to_string()
        };
        // The listener speaks HTTP CONNECT, not SOCKS; leave all SOCKS settings untouched.
        for protocol in ["HTTP", "HTTPS"] {
            proxies.insert(format!("{protocol}Enable"), 1.into());
            proxies.insert(format!("{protocol}Proxy"), host.clone().into());
            proxies.insert(format!("{protocol}Port"), u64::from(addr.port()).into());
        }
        for key in ["ProxyAutoConfigEnable", "ProxyAutoDiscoveryEnable"] {
            if proxies.contains_key(key) {
                proxies.insert(key.into(), 0.into());
            }
        }
        let mut xml = Vec::new();
        plist::Value::Dictionary(proxies).to_writer_xml(&mut xml)?;
        Ok(Self {
            service_id: self.service_id.clone(),
            proxies_xml: Some(String::from_utf8(xml)?),
        })
    }
    pub(crate) fn write(&self, expected: &Self) -> Result<()> {
        ensure!(
            self.service_id == expected.service_id,
            "network service identity changed"
        );
        let authorization = Authorization::new()?;
        let prefs = open_preferences(Some(&authorization))?;
        checked(
            unsafe { SCPreferencesLock(prefs.as_concrete_TypeRef(), 1) },
            "lock network preferences",
        )?;
        let _lock = PreferencesLock(&prefs);
        ensure!(
            Self::read_for(&prefs, &self.service_id)? == *expected,
            "system proxy changed before writing; recovery record retained"
        );
        let path = self.path()?;
        if let Some(xml) = &self.proxies_xml {
            let (raw, _) = propertylist::create_with_data(
                CFData::from_buffer(xml.as_bytes()),
                propertylist::kCFPropertyListImmutable,
            )
            .map_err(|e| anyhow::anyhow!("invalid proxy snapshot: {e}"))?;
            // create_with_data transfers ownership; the CF wrapper releases it after the setter copies it.
            let value = unsafe { CFPropertyList::wrap_under_create_rule(raw) };
            checked(
                unsafe {
                    SCPreferencesPathSetValue(
                        prefs.as_concrete_TypeRef(),
                        path.as_concrete_TypeRef(),
                        value.as_concrete_TypeRef(),
                    )
                },
                "set network proxies",
            )?;
        } else {
            checked(
                unsafe {
                    SCPreferencesPathRemoveValue(
                        prefs.as_concrete_TypeRef(),
                        path.as_concrete_TypeRef(),
                    )
                },
                "remove network proxies",
            )?;
        }
        checked(
            unsafe { SCPreferencesCommitChanges(prefs.as_concrete_TypeRef()) },
            "commit network proxies",
        )?;
        checked(
            unsafe { SCPreferencesApplyChanges(prefs.as_concrete_TypeRef()) },
            "apply network proxies",
        )
    }
}
fn plist_value(raw: propertylist::CFPropertyListRef) -> Result<plist::Value> {
    let data = propertylist::create_data(raw, propertylist::kCFPropertyListXMLFormat_v1_0)
        .map_err(|e| anyhow::anyhow!("invalid system property list: {e}"))?;
    Ok(plist::Value::from_reader(std::io::Cursor::new(
        data.bytes(),
    ))?)
}
fn checked(success: u8, operation: &str) -> Result<()> {
    ensure!(
        success != 0,
        "cannot {operation} (SystemConfiguration error {})",
        unsafe { SCError() }
    );
    Ok(())
}
fn open_preferences(authorization: Option<&Authorization>) -> Result<SCPreferences> {
    let name = CFString::new("Proxy Fork");
    let raw = unsafe {
        match authorization {
            Some(auth) => SCPreferencesCreateWithAuthorization(
                ptr::null(),
                name.as_concrete_TypeRef(),
                ptr::null(),
                auth.0,
            ),
            None => SCPreferencesCreate(ptr::null(), name.as_concrete_TypeRef(), ptr::null()),
        }
    };
    ensure!(!raw.is_null(), "cannot open network preferences");
    Ok(unsafe { SCPreferences::wrap_under_create_rule(raw) })
}
struct PreferencesLock<'a>(&'a SCPreferences);
impl Drop for PreferencesLock<'_> {
    fn drop(&mut self) {
        unsafe {
            SCPreferencesUnlock(self.0.as_concrete_TypeRef());
        }
    }
}
struct Authorization(auth::AuthorizationRef);
impl Authorization {
    fn new() -> Result<Self> {
        let mut handle = ptr::null_mut();
        let status = unsafe {
            auth::AuthorizationCreate(
                ptr::null(),
                ptr::null(),
                auth::kAuthorizationFlagDefaults,
                &mut handle,
            )
        };
        ensure!(status == 0, "network authorization failed: {status}");
        let authorization = Self(handle);
        let mut right = auth::AuthorizationItem {
            name: c"system.preferences.network".as_ptr(),
            valueLength: 0,
            value: ptr::null_mut(),
            flags: 0,
        };
        let rights = auth::AuthorizationRights {
            count: 1,
            items: &mut right,
        };
        let status = unsafe {
            auth::AuthorizationCopyRights(
                handle,
                &rights,
                ptr::null(),
                auth::kAuthorizationFlagInteractionAllowed | auth::kAuthorizationFlagExtendRights,
                ptr::null_mut(),
            )
        };
        ensure!(
            status == 0,
            "network authorization denied or cancelled: {status}"
        );
        Ok(authorization)
    }
}
impl Drop for Authorization {
    fn drop(&mut self) {
        unsafe {
            auth::AuthorizationFree(self.0, auth::kAuthorizationFlagDefaults);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "opt-in read-only host network snapshot smoke test"]
    fn current_network_snapshot_round_trips_without_writing_system_settings() {
        let snapshot = ProxySettings::read().unwrap();
        let encoded = toml::to_string(&snapshot).unwrap();
        assert!(toml::from_str::<ProxySettings>(&encoded).unwrap() == snapshot);
        assert!(snapshot.read_current().unwrap() == snapshot);
    }

    #[test]
    fn native_failure_status_and_malformed_service_ids_are_rejected() {
        assert!(checked(0, "restore network proxies").is_err());
        assert!(checked(1, "restore network proxies").is_ok());
        assert!(
            ProxySettings {
                service_id: "../another-service".into(),
                proxies_xml: None
            }
            .path()
            .is_err()
        );
    }

    #[test]
    fn snapshot_keeps_service_identity_and_independent_protocols() {
        let mut proxies = plist::Dictionary::new();
        proxies.insert("HTTPEnable".into(), 1.into());
        proxies.insert("HTTPProxy".into(), "corporate.example".into());
        proxies.insert("HTTPPort".into(), 8080.into());
        proxies.insert("HTTPSEnable".into(), 0.into());
        proxies.insert("SOCKSEnable".into(), 1.into());
        proxies.insert("SOCKSProxy".into(), "socks.example".into());
        proxies.insert("SOCKSPort".into(), 1080.into());
        let mut xml = Vec::new();
        plist::Value::Dictionary(proxies.clone())
            .to_writer_xml(&mut xml)
            .unwrap();
        let original = ProxySettings {
            service_id: "D5EB6794-0CF4-4DDF-B56F-574587170537".into(),
            proxies_xml: Some(String::from_utf8(xml).unwrap()),
        };
        let written = original
            .for_listener("127.0.0.1:7898".parse().unwrap())
            .unwrap();
        assert_eq!(written.service_id, original.service_id);
        let parsed =
            plist::Value::from_reader(std::io::Cursor::new(written.proxies_xml.as_ref().unwrap()))
                .unwrap();
        let changed = parsed.as_dictionary().unwrap();
        assert_eq!(changed["HTTPProxy"].as_string(), Some("127.0.0.1"));
        assert_eq!(changed["HTTPSPort"].as_unsigned_integer(), Some(7898));
        assert_eq!(changed["SOCKSProxy"], proxies["SOCKSProxy"]);
        assert_eq!(changed["SOCKSEnable"], proxies["SOCKSEnable"]);
        let record = toml::to_string(&original).unwrap();
        assert_eq!(toml::from_str::<ProxySettings>(&record).unwrap(), original);
        let original_dict =
            plist::Value::from_reader(std::io::Cursor::new(original.proxies_xml.as_ref().unwrap()))
                .unwrap();
        assert_eq!(original_dict.as_dictionary().unwrap(), &proxies);
    }
}
