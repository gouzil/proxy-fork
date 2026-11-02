use hudsucker::openssl::{
    asn1::Asn1Time,
    bn::BigNum,
    hash::MessageDigest,
    pkey::PKey,
    x509::{X509, X509NameBuilder},
};
use proxy_fork_core::{certificate_fingerprint, certificate_info};
use time::Month;

#[test]
fn certificate_details_come_from_the_selected_pem_or_der_file() {
    let key = PKey::generate_ed25519().unwrap();
    let mut subject = X509NameBuilder::new().unwrap();
    subject.append_entry_by_text("CN", "Display CA").unwrap();
    subject.append_entry_by_text("O", "证书实验室").unwrap();
    let mut issuer = X509NameBuilder::new().unwrap();
    issuer.append_entry_by_text("CN", "Fixture Issuer").unwrap();
    let mut certificate = X509::builder().unwrap();
    certificate.set_version(2).unwrap();
    certificate.set_subject_name(&subject.build()).unwrap();
    certificate.set_issuer_name(&issuer.build()).unwrap();
    certificate.set_pubkey(&key).unwrap();
    certificate
        .set_serial_number(&BigNum::from_u32(0x1234).unwrap().to_asn1_integer().unwrap())
        .unwrap();
    certificate
        .set_not_before(&Asn1Time::from_str_x509("20261102062454Z").unwrap())
        .unwrap();
    certificate
        .set_not_after(&Asn1Time::from_str_x509("20271102062454Z").unwrap())
        .unwrap();
    certificate.sign(&key, MessageDigest::null()).unwrap();
    let certificate = certificate.build();
    let dir = tempfile::tempdir().unwrap();
    for bytes in [certificate.to_pem().unwrap(), certificate.to_der().unwrap()] {
        let path = dir.path().join("certificate");
        fs_err::write(&path, bytes).unwrap();
        let info = certificate_info(&path).unwrap();
        assert!(info.subject.contains("CN=Display CA"));
        assert!(info.subject.contains("O=证书实验室"));
        assert_eq!(info.issuer, "CN=Fixture Issuer");
        assert_eq!(info.serial_number, "12:34");
        assert_eq!(
            info.not_before.to_calendar_date(),
            (2026, Month::November, 2)
        );
        assert_eq!(info.not_before.to_hms(), (6, 24, 54));
        assert_eq!(
            info.not_after.to_calendar_date(),
            (2027, Month::November, 2)
        );
        assert_eq!(info.not_after.to_hms(), (6, 24, 54));
        assert_eq!(info.fingerprint, certificate_fingerprint(&path).unwrap());
    }
    assert!(certificate_info(&dir.path().join("missing")).is_err());
    let invalid = dir.path().join("invalid");
    fs_err::write(&invalid, b"not a certificate").unwrap();
    assert!(certificate_info(&invalid).is_err());
}
