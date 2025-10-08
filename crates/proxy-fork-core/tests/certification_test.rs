#[cfg(test)]
mod certification_test {
    use hudsucker::openssl::{self, pkey::PKey, x509::X509};
    use openssl::{
        hash::MessageDigest,
        sign::{Signer, Verifier},
    };
    use proxy_fork_core::certification::{CertInput, load_ca_from_sources, load_cert};
    use proxy_fork_core::certification::{SelfSignedCa, SelfSignedCaBuilder, load_cert_from_file};

    #[tokio::test]
    async fn test_gen_ca() {
        let ca_name = "Proxy-Fork CA Test";
        let self_signed_builder = SelfSignedCaBuilder::default()
            .ca_name(ca_name)
            .build()
            .unwrap();
        let self_signed_ca = SelfSignedCa::gen_signed_cert(&self_signed_builder).unwrap();
        let ca_cert = X509::from_der(self_signed_ca.certificate.der()).unwrap();
        assert!(ca_cert.subject_name().entries().next().is_some());
        // 确认名称正确
        assert!(
            ca_cert
                .subject_name()
                .entries()
                .next()
                .unwrap()
                .data()
                .as_utf8()
                .unwrap()
                .to_string()
                == ca_name
        );
        // 确认证书有效期
        let not_before = ca_cert.not_before();
        let not_after = ca_cert.not_after();
        assert!(not_before < not_after);
        // 确认现在是在有效期内
        let now = openssl::asn1::Asn1Time::days_from_now(0).unwrap();
        assert!(now >= *not_before);
        assert!(now <= *not_after);

        // 确认私钥存在
        assert!(!self_signed_ca.issuer.key().serialize_der().is_empty());

        // 确认私钥和公钥匹配，可以用公钥验证私钥签名的数据
        let public_key = ca_cert.public_key().unwrap();
        let data = b"test data";
        // 将 rcgen::KeyPair 转换为 openssl::PKey
        let private_key =
            PKey::private_key_from_der(&self_signed_ca.issuer.key().serialize_der()).unwrap();
        let mut signer = Signer::new(MessageDigest::sha256(), &private_key).unwrap();
        signer.update(data).unwrap();
        let signature = signer.sign_to_vec().unwrap();
        let mut verifier = Verifier::new(MessageDigest::sha256(), &public_key).unwrap();
        verifier.update(data).unwrap();
        assert!(verifier.verify(&signature).unwrap());
    }

    #[tokio::test]
    async fn test_local_cert() {
        // 生成临时证书文件
        let tmpdir = tempfile::tempdir().unwrap();
        let file_path = tmpdir.path().join("proxy-fork-ca.pem");

        let ca_name = "Proxy-Fork CA Temp";
        let self_signed_builder = SelfSignedCaBuilder::default()
            .ca_name(ca_name)
            .build()
            .unwrap();
        let self_signed_ca = SelfSignedCa::gen_signed_cert(&self_signed_builder).unwrap();

        // 转换为 PEM 格式并写入文件
        let ca_cert = X509::from_der(self_signed_ca.certificate.der()).unwrap();
        let pem_bytes = ca_cert.to_pem().unwrap();
        std::fs::write(&file_path, &pem_bytes).unwrap();

        // 从文件加载证书
        let cert_bytes = load_cert_from_file(file_path.to_str().unwrap());
        assert!(cert_bytes.is_some());
        let cert_loaded = X509::from_pem(&cert_bytes.unwrap()).unwrap();

        // 确认加载的证书和原始证书一致
        assert_eq!(cert_loaded.to_der().unwrap(), ca_cert.to_der().unwrap());

        // 确认私钥和公钥匹配，可以用公钥验证私钥签名的数据
        let public_key = ca_cert.public_key().unwrap();
        let data = b"test data";
        // 将 rcgen::KeyPair 转换为 openssl::PKey
        let private_key =
            PKey::private_key_from_der(&self_signed_ca.issuer.key().serialize_der()).unwrap();
        let mut signer = Signer::new(MessageDigest::sha256(), &private_key).unwrap();
        signer.update(data).unwrap();
        let signature = signer.sign_to_vec().unwrap();
        let mut verifier = Verifier::new(MessageDigest::sha256(), &public_key).unwrap();
        verifier.update(data).unwrap();
        assert!(verifier.verify(&signature).unwrap());
    }

    #[tokio::test]
    async fn test_load_cert_variants_and_ca_loader() {
        // 生成临时 self-signed CA
        let ca_name = "Proxy-Fork CA For Loader";
        let self_signed_builder = SelfSignedCaBuilder::default()
            .ca_name(ca_name)
            .build()
            .unwrap();
        let self_signed_ca = SelfSignedCa::gen_signed_cert(&self_signed_builder).unwrap();

        // 将证书转换为 PEM
        let ca_cert = X509::from_der(self_signed_ca.certificate.der()).unwrap();
        let pem_bytes = ca_cert.to_pem().unwrap();

        // 1) load_cert from Bytes
        let loaded = load_cert(CertInput::Bytes(pem_bytes.clone())).unwrap();
        assert_eq!(loaded, pem_bytes);

        // 2) load_cert from File
        let tmpdir = tempfile::tempdir().unwrap();
        let cert_path = tmpdir.path().join("loader-ca.pem");
        std::fs::write(&cert_path, &pem_bytes).unwrap();
        let loaded_file = load_cert(CertInput::File(cert_path.to_str().unwrap())).unwrap();
        assert_eq!(loaded_file, pem_bytes);

        // 3) load_cert returns Err for missing file
        let missing = load_cert(CertInput::File("/non/existent/cert.pem"));
        assert!(missing.is_err());

        // 4) load_ca_from_sources using cert file + private key bytes
        let key_der = self_signed_ca.issuer.key().serialize_der();
        let ca_loader = load_ca_from_sources(
            CertInput::File(cert_path.to_str().unwrap()),
            CertInput::Bytes(key_der.clone()),
        );
        assert!(ca_loader.is_ok());
    }
}
