use crate::error::{ApiResult, AppError};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::Engine as _;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use totp_rs::{Algorithm, Secret, TOTP};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct CryptoContext {
    master_key: [u8; 32],
    jwt_secret: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub username: String,
    pub global_role: String,
    pub scope: String, // "access" | "mfa_pending"
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TotpSetupInfo {
    pub secret_base32: String,
    pub otpauth_uri: String,
    pub qr_data_uri: String,
}

impl CryptoContext {
    pub fn from_bytes(master_key: [u8; 32]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"tss-jwt-signing-v1");
        hasher.update(master_key);
        let jwt_secret: [u8; 32] = hasher.finalize().into();
        Self {
            master_key,
            jwt_secret,
        }
    }

    pub fn load_or_create(data_dir: &Path, env_key: Option<&str>) -> ApiResult<Self> {
        if let Some(raw) = env_key.map(str::trim).filter(|s| !s.is_empty()) {
            let key = parse_master_key(raw)?;
            return Ok(Self::from_bytes(key));
        }

        std::fs::create_dir_all(data_dir)
            .map_err(|e| AppError::Internal(format!("create data dir failed: {e}")))?;
        let key_path = data_dir.join(".master.key");
        if key_path.exists() {
            let content = std::fs::read_to_string(&key_path)
                .map_err(|e| AppError::Internal(format!("read master key failed: {e}")))?;
            let key = parse_master_key(content.trim())?;
            return Ok(Self::from_bytes(key));
        }

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let hex_key = hex::encode(key);
        std::fs::write(&key_path, &hex_key)
            .map_err(|e| AppError::Internal(format!("write master key failed: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(Self::from_bytes(key))
    }

    pub fn hash_password(&self, password: &str) -> ApiResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Internal(format!("Argon2 hash failed: {e}")))?
            .to_string();
        Ok(hash)
    }

    pub fn verify_password(&self, password: &str, hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }

    pub fn encrypt_secret(&self, plaintext: &str) -> ApiResult<String> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.master_key)
            .map_err(|e| AppError::Internal(format!("AEAD init failed: {e}")))?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| AppError::Internal(format!("AEAD encrypt failed: {e}")))?;
        let mut combined = Vec::with_capacity(12 + ct.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ct);
        let encoded = base64::engine::general_purpose::STANDARD.encode(combined);
        Ok(format!("v1:{encoded}"))
    }

    pub fn decrypt_secret(&self, stored: &str) -> ApiResult<String> {
        let b64 = stored.strip_prefix("v1:").unwrap_or(stored);
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|e| AppError::Internal(format!("AEAD base64 decode failed: {e}")))?;
        if raw.len() < 13 {
            return Err(AppError::Internal("AEAD payload too short".to_string()));
        }
        let (nonce_bytes, ct) = raw.split_at(12);
        let cipher = ChaCha20Poly1305::new_from_slice(&self.master_key)
            .map_err(|e| AppError::Internal(format!("AEAD init failed: {e}")))?;
        let pt = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ct)
            .map_err(|_| AppError::Internal("AEAD decrypt failed (key mismatch or tampered)".to_string()))?;
        String::from_utf8(pt)
            .map_err(|e| AppError::Internal(format!("AEAD utf8 decode failed: {e}")))
    }

    pub fn issue_access_jwt(&self, user_id: &str, username: &str, global_role: &str) -> ApiResult<String> {
        self.issue_jwt(user_id, username, global_role, "access", 900)
    }

    pub fn issue_mfa_ticket(&self, user_id: &str, username: &str, global_role: &str) -> ApiResult<String> {
        self.issue_jwt(user_id, username, global_role, "mfa_pending", 300)
    }

    fn issue_jwt(
        &self,
        user_id: &str,
        username: &str,
        global_role: &str,
        scope: &str,
        ttl_secs: i64,
    ) -> ApiResult<String> {
        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            sub: user_id.to_string(),
            username: username.to_string(),
            global_role: global_role.to_string(),
            scope: scope.to_string(),
            iat: now,
            exp: now + ttl_secs,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| AppError::Internal(format!("JWT encode failed: {e}")))
    }

    pub fn verify_jwt(&self, token: &str, expected_scope: &str) -> ApiResult<JwtClaims> {
        let data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
        )
        .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))?;
        if data.claims.scope != expected_scope {
            return Err(AppError::Unauthorized("Invalid token scope".to_string()));
        }
        Ok(data.claims)
    }

    pub fn generate_refresh_token(&self) -> (String, String) {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let hash = sha256_hex(raw.as_bytes());
        (raw, hash)
    }

    pub fn generate_totp_setup(&self, username: &str) -> ApiResult<TotpSetupInfo> {
        let secret = Secret::generate_secret();
        let secret_base32 = secret.to_encoded().to_string();
        let safe_account = username.replace(':', "_");
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret
                .to_bytes()
                .map_err(|e| AppError::Internal(format!("TOTP secret bytes failed: {e}")))?,
            Some("TSS-Manager".to_string()),
            safe_account,
        )
        .map_err(|e| AppError::Internal(format!("TOTP init failed: {e}")))?;

        let otpauth_uri = totp.get_url();
        let qr_b64 = totp
            .get_qr_base64()
            .map_err(|e| AppError::Internal(format!("TOTP QR failed: {e}")))?;
        Ok(TotpSetupInfo {
            secret_base32,
            otpauth_uri,
            qr_data_uri: format!("data:image/png;base64,{qr_b64}"),
        })
    }

    pub fn verify_totp_code(&self, secret_base32: &str, username: &str, code: &str) -> ApiResult<bool> {
        let secret = Secret::Encoded(secret_base32.trim().to_string());
        let bytes = secret
            .to_bytes()
            .map_err(|e| AppError::Validation(format!("Invalid TOTP secret: {e}")))?;
        let safe_account = username.replace(':', "_");
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            bytes,
            Some("TSS-Manager".to_string()),
            safe_account,
        )
        .map_err(|e| AppError::Internal(format!("TOTP init failed: {e}")))?;
        totp.check_current(code.trim())
            .map_err(|e| AppError::Internal(format!("TOTP check failed: {e}")))
    }

    pub fn hmac_sha256_hex(&self, secret: &str, payload: &[u8]) -> String {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes())
            .expect("HMAC accepts any key size");
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn parse_master_key(raw: &str) -> ApiResult<[u8; 32]> {
    if raw.len() == 64 && raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        let bytes = hex::decode(raw)
            .map_err(|e| AppError::Internal(format!("Invalid hex master key: {e}")))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| AppError::Internal("Master key must be 32 bytes".to_string()))?;
        return Ok(arr);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.as_bytes())
        .map_err(|e| AppError::Internal(format!("Invalid base64 master key: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| AppError::Internal("Master key must be 32 bytes".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_and_aead_roundtrip() {
        let ctx = CryptoContext::from_bytes([7u8; 32]);
        let hash = ctx.hash_password("S3cur3-Pass!").unwrap();
        assert!(ctx.verify_password("S3cur3-Pass!", &hash));
        assert!(!ctx.verify_password("wrong", &hash));

        let enc = ctx.encrypt_secret("ts-secret-key").unwrap();
        assert!(enc.starts_with("v1:"));
        let dec = ctx.decrypt_secret(&enc).unwrap();
        assert_eq!(dec, "ts-secret-key");
    }

    #[test]
    fn jwt_and_totp_roundtrip() {
        let ctx = CryptoContext::from_bytes([9u8; 32]);
        let token = ctx.issue_access_jwt("u1", "admin", "super_admin").unwrap();
        let claims = ctx.verify_jwt(&token, "access").unwrap();
        assert_eq!(claims.sub, "u1");
        assert!(ctx.verify_jwt(&token, "mfa_pending").is_err());

        let setup = ctx.generate_totp_setup("admin").unwrap();
        assert!(setup.otpauth_uri.starts_with("otpauth://totp/"));
        assert!(setup.qr_data_uri.starts_with("data:image/png;base64,"));
        let secret = Secret::Encoded(setup.secret_base32.clone()).to_bytes().unwrap();
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret, Some("TSS-Manager".into()), "admin".into()).unwrap();
        let code = totp.generate_current().unwrap();
        assert!(ctx.verify_totp_code(&setup.secret_base32, "admin", &code).unwrap());
    }
}
