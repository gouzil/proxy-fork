use std::sync::Arc;

use derive_builder::Builder;
use fs_err as fs;
use http::uri::Authority;
use hudsucker::{
    certificate_authority::{CertificateAuthority, OpensslAuthority},
    openssl::{hash::MessageDigest, pkey::PKey, x509::X509},
    rcgen::{
        self, Certificate, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    },
    rustls::{ServerConfig, crypto::aws_lc_rs},
};
use std::error::Error;
use time::{Duration, OffsetDateTime};
use tracing::error;
use x509_parser::prelude::parse_x509_certificate;

// 证书颁发机构枚举，支持 OpenSSL 和无证书两种模式
pub enum CaEnum {
    Openssl(OpensslAuthority),
    None(NoCa),
}

// 证书颁发机构枚举构造器
impl CertificateAuthority for CaEnum {
    async fn gen_server_config(
        &self,
        authority: &http::uri::Authority,
    ) -> std::sync::Arc<hudsucker::rustls::ServerConfig> {
        match self {
            CaEnum::Openssl(ca) => ca.gen_server_config(authority).await,
            CaEnum::None(ca) => ca.gen_server_config(authority).await,
        }
    }
}

/// 证书输入抽象，支持从系统证书（按 Common Name 匹配）、文件或内存字节加载
pub enum CertInput<'a> {
    System(&'a str),
    File(&'a str),
    Bytes(Vec<u8>),
}

/// 通用证书加载器：根据 `CertInput` 返回原始字节 (通常是 DER 或 PEM)
/// 注意：函数不尝试转换 PEM <-> DER，调用者应根据需要解析或转换字节。
pub fn load_cert(source: CertInput) -> Result<Vec<u8>, Box<dyn Error>> {
    match source {
        CertInput::System(name) => {
            if let Some(bytes) = get_system_cert_by_name(name) {
                Ok(bytes)
            } else {
                Err(format!("no system certificate found with CN=\"{}\"", name).into())
            }
        }
        CertInput::File(path) => match load_cert_from_file(path) {
            Some(bytes) => Ok(bytes),
            None => Err(format!("failed to read certificate file: {}", path).into()),
        },
        CertInput::Bytes(bytes) => Ok(bytes),
    }
}

/// 获取系统指定名称证书
pub fn get_system_cert_by_name(ca_name: &str) -> Option<Vec<u8>> {
    for cert in rustls_native_certs::load_native_certs().expect("could not load platform certs") {
        match parse_x509_certificate(cert.as_ref()) {
            Ok((_, cert_)) => {
                let cn = cert_
                    .subject()
                    .iter_common_name()
                    .next()
                    .and_then(|cn| cn.as_str().ok());
                if let Some(cn) = cn
                    && cn == ca_name
                {
                    return Some(cert.as_ref().to_vec());
                }
            }
            Err(e) => error!("error parsing certificate: {}", e),
        };
    }
    None
}

/// 加载本地证书文件
pub fn load_cert_from_file(path: &str) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(data) => Some(data),
        Err(e) => {
            error!("Failed to read certificate file {}: {}", path, e);
            None
        }
    }
}

/// 从给定的证书/私钥来源加载并构造一个 `OpensslAuthority`。
/// 支持 DER/PEM 格式的证书与私钥字节；对于证书会先尝试按 DER 解析然后尝试 PEM。
pub fn load_ca_from_sources(
    cert_src: CertInput,
    key_src: CertInput,
) -> Result<OpensslAuthority, Box<dyn Error>> {
    // 证书字节
    let cert_bytes = load_cert(cert_src)?;
    let ca_cert = match X509::from_der(&cert_bytes) {
        Ok(c) => c,
        Err(_) => X509::from_pem(&cert_bytes)?,
    };

    // 私钥字节（System 来源不适用于私钥）
    let key_bytes = match key_src {
        CertInput::System(name) => {
            return Err(format!(
                "cannot load private key from system certs by name: {}",
                name
            )
            .into());
        }
        CertInput::File(path) => load_cert(CertInput::File(path))?,
        CertInput::Bytes(b) => b,
    };

    let private_key = PKey::private_key_from_pem(&key_bytes)
        .or_else(|_| PKey::private_key_from_der(&key_bytes))?;

    Ok(OpensslAuthority::new(
        private_key,
        ca_cert,
        MessageDigest::sha256(),
        1_000,
        aws_lc_rs::default_provider(),
    ))
}

// 无证书
pub struct NoCa;

impl CertificateAuthority for NoCa {
    async fn gen_server_config(&self, _authority: &Authority) -> Arc<ServerConfig> {
        unreachable!();
    }
}

// ====== 自签名证书 =======
#[derive(Builder)]
#[builder(pattern = "owned", name = "SelfSignedCaBuilder")]
pub struct SelfSignedCaConfig<'a> {
    #[builder(default = "\"Proxy-Fork CA\"")]
    pub ca_name: &'a str,
    #[builder(default = "365")]
    pub validity_days: i64,
}

// copy from rcgen::CertifiedIssuer
pub struct SelfSignedCa {
    pub certificate: Certificate,
    pub issuer: Issuer<'static, KeyPair>,
}
impl SelfSignedCa {
    /// 生成 CA 证书和私钥
    pub fn gen_signed_cert(
        self_builder: &SelfSignedCaConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // 设置证书参数
        let mut params = match CertificateParams::new(vec![]) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to create certificate params: {}", e);
                return Err(Box::new(e));
            }
        };

        params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, self_builder.ca_name);

        // 设定证书有效期
        let now = OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + Duration::days(self_builder.validity_days);

        // 生成密钥对
        let key_pair = match KeyPair::generate() {
            Ok(kp) => kp,
            Err(e) => {
                error!("Failed to generate key pair: {}", e);
                return Err(Box::new(e));
            }
        };

        // 生成自签名证书
        let ca = match params.self_signed(&key_pair) {
            Ok(cert) => cert,
            Err(e) => {
                error!("Failed to generate self-signed certificate: {}", e);
                return Err(Box::new(e));
            }
        };

        let issuer = Issuer::new(params, key_pair);

        Ok(Self {
            certificate: ca,
            issuer,
        })
    }
}
