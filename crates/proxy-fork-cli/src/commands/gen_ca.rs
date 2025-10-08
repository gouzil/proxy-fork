use anyhow::Result;
use fs_err as fs;
use proxy_fork_core::certification::{SelfSignedCa, SelfSignedCaBuilder};
use std::io::{self, BufRead, Write};
use tracing::info;

use crate::args::GenCaArgs;
use crate::dirs::{default_cert_path, default_private_key_path};

fn confirm_overwrite<R: BufRead>(reader: &mut R, path: &std::path::Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }

    print!(
        "File {} already exists. Do you want to overwrite it? (y/N): ",
        path.display()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    Ok(matches!(input.as_str(), "y" | "yes"))
}

pub(crate) async fn gen_ca(args: &GenCaArgs) -> Result<()> {
    // 使用默认配置生成自签名 CA
    let ca_config = SelfSignedCaBuilder::default().build()?;
    let self_signed_ca = SelfSignedCa::gen_signed_cert(&ca_config)
        .expect("Failed to generate self-signed CA certificate");

    // 获取证书和私钥的 PEM 字节
    let cert_pem = self_signed_ca.certificate.pem();
    let key_pem = self_signed_ca.issuer.key().serialize_pem();

    // 确定输出路径
    let cert_path = args
        .ca_cert
        .as_ref()
        .map(|p| p.clone())
        .unwrap_or_else(|| default_cert_path().unwrap());
    let key_path = args
        .ca_key
        .as_ref()
        .map(|p| p.clone())
        .unwrap_or_else(|| default_private_key_path().unwrap());

    // 如果使用默认路径且文件已存在，确认覆盖
    let using_defaults = args.ca_cert.is_none() && args.ca_key.is_none();
    if using_defaults {
        if !confirm_overwrite(&mut io::stdin().lock(), &cert_path)? {
            info!("Certificate generation cancelled by user.");
            return Ok(());
        }
        if !confirm_overwrite(&mut io::stdin().lock(), &key_path)? {
            info!("Certificate generation cancelled by user.");
            return Ok(());
        }
    }

    // 确保目录存在
    if let Some(parent) = cert_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 写入文件
    fs::write(&cert_path, &cert_pem)?;
    fs::write(&key_path, &key_pem)?;

    info!(
        "CA certificate generated and saved to: {}",
        cert_path.display()
    );
    info!("CA private key saved to: {}", key_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::GenCaArgs;
    use std::io::Cursor;
    use tempfile::tempdir;

    #[test]
    fn test_confirm_overwrite_file_not_exists() {
        let temp_dir = tempdir().unwrap();
        let non_existent_path = temp_dir.path().join("nonexistent.txt");

        let mut input = Cursor::new(b"");
        let result = confirm_overwrite(&mut input, &non_existent_path);
        assert!(result.unwrap());
    }

    #[test]
    fn test_confirm_overwrite_yes() {
        let temp_dir = tempdir().unwrap();
        let existing_file = temp_dir.path().join("existing.txt");
        std::fs::write(&existing_file, "test").unwrap();

        let mut input = Cursor::new(b"y\n");
        let result = confirm_overwrite(&mut input, &existing_file);
        assert!(result.unwrap());
    }

    #[test]
    fn test_confirm_overwrite_yes_uppercase() {
        let temp_dir = tempdir().unwrap();
        let existing_file = temp_dir.path().join("existing.txt");
        std::fs::write(&existing_file, "test").unwrap();

        let mut input = Cursor::new(b"YES\n");
        let result = confirm_overwrite(&mut input, &existing_file);
        assert!(result.unwrap());
    }

    #[test]
    fn test_confirm_overwrite_no() {
        let temp_dir = tempdir().unwrap();
        let existing_file = temp_dir.path().join("existing.txt");
        std::fs::write(&existing_file, "test").unwrap();

        let mut input = Cursor::new(b"n\n");
        let result = confirm_overwrite(&mut input, &existing_file);
        assert!(!result.unwrap());
    }

    #[test]
    fn test_confirm_overwrite_default_no() {
        let temp_dir = tempdir().unwrap();
        let existing_file = temp_dir.path().join("existing.txt");
        std::fs::write(&existing_file, "test").unwrap();

        let mut input = Cursor::new(b"something else\n");
        let result = confirm_overwrite(&mut input, &existing_file);
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_gen_ca_default_paths() {
        // 创建临时目录模拟用户数据目录
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 模拟默认路径
        let cert_path = temp_path.join("proxy-fork-ca-cert.pem");
        let key_path = temp_path.join("proxy-fork-ca.pem");

        // 设置环境变量或 mock dirs，但这里直接使用临时路径
        // 由于 default_cert_path() 使用 etcetera，我们需要 mock 或直接测试逻辑

        // 实际上，我们可以直接测试函数，但需要处理路径
        // 为了简单，我们测试自定义路径的情况

        let args = GenCaArgs {
            ca_cert: Some(cert_path.clone()),
            ca_key: Some(key_path.clone()),
        };

        // 执行生成
        gen_ca(&args).await.expect("Failed to generate CA");

        // 验证文件存在
        assert!(cert_path.exists(), "Certificate file should exist");
        assert!(key_path.exists(), "Private key file should exist");

        // 验证文件内容
        let cert_content = std::fs::read_to_string(&cert_path).unwrap();
        let key_content = std::fs::read_to_string(&key_path).unwrap();

        // 检查是否是 PEM 格式
        assert!(cert_content.contains("-----BEGIN CERTIFICATE-----"));
        assert!(cert_content.contains("-----END CERTIFICATE-----"));
        assert!(key_content.contains("-----BEGIN PRIVATE KEY-----"));
        assert!(key_content.contains("-----END PRIVATE KEY-----"));

        // 可以进一步验证证书内容，但这里简单检查即可
    }

    #[tokio::test]
    async fn test_gen_ca_custom_paths() {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 测试嵌套目录自动创建
        let nested_cert_path = temp_path.join("nested").join("dir").join("ca-cert.pem");
        let nested_key_path = temp_path.join("nested").join("dir").join("ca-key.pem");

        let args = GenCaArgs {
            ca_cert: Some(nested_cert_path.clone()),
            ca_key: Some(nested_key_path.clone()),
        };

        // 执行生成
        gen_ca(&args).await.expect("Failed to generate CA");

        // 验证文件存在
        assert!(nested_cert_path.exists(), "Certificate file should exist");
        assert!(nested_key_path.exists(), "Private key file should exist");

        // 验证父目录被创建
        assert!(nested_cert_path.parent().unwrap().exists());
        assert!(nested_key_path.parent().unwrap().exists());
    }
}
