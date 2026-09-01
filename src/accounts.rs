use crate::config::NotificationSettings;
use crate::database::Database;
use crate::tenant::{TenantDefinition, TenantId, TenantRepository};
use crate::util::{generate_secure_token, now_unix_secs, stream_key_digest};
use anyhow::{Context, Result, bail};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};
use topcoat::session::{Session, TokenHash};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Admin,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "admin" => Ok(Self::Admin),
            "user" => Ok(Self::User),
            _ => bail!("Stored user role is invalid"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    pub id: String,
    pub tenant_id: TenantId,
    pub email: String,
    pub role: Role,
}

#[derive(Clone)]
pub struct AccountRepository {
    database: Database,
    tenants: TenantRepository,
}

impl AccountRepository {
    pub fn new(database: Database) -> Self {
        Self {
            tenants: TenantRepository::new(database.clone()),
            database,
        }
    }

    pub async fn has_users(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(self.database.pool())
            .await?;
        Ok(count > 0)
    }

    pub async fn create_local_user(
        &self,
        tenant_id: &TenantId,
        email: &str,
        password: &str,
        role: Role,
    ) -> Result<User> {
        let email = normalize_email(email)?;
        validate_password(password)?;
        let password_hash = hash_password(password.to_owned()).await?;
        let user = User {
            id: generate_secure_token()?,
            tenant_id: tenant_id.clone(),
            email,
            role,
        };
        sqlx::query(
            "INSERT INTO users (id, tenant_id, email, password_hash, role, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&user.id)
        .bind(user.tenant_id.as_str())
        .bind(&user.email)
        .bind(password_hash)
        .bind(user.role.as_str())
        .bind(i64::try_from(now_unix_secs())?)
        .execute(self.database.pool())
        .await
        .context("Failed to create user account")?;
        Ok(user)
    }

    pub async fn register(&self, email: &str, password: &str) -> Result<(User, String)> {
        let tenant_id = TenantId::new(generate_secure_token()?)?;
        let stream_key = generate_secure_token()?;
        self.tenants
            .save(TenantDefinition {
                id: &tenant_id,
                name: email.trim(),
                stream_key: &stream_key,
                active: true,
                max_concurrent_streams: 1,
                notifications: &NotificationSettings::default(),
                chat: &crate::config::ChatSettings::default(),
                overlay: &crate::config::OverlaySettings::default(),
                targets: &[],
            })
            .await?;
        let user = self
            .create_local_user(&tenant_id, email, password, Role::User)
            .await?;
        Ok((user, stream_key))
    }

    pub async fn authenticate_password(&self, email: &str, password: &str) -> Result<Option<User>> {
        let email = normalize_email(email)?;
        let row = sqlx::query(
            "SELECT id, tenant_id, email, password_hash, role FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(self.database.pool())
        .await?;
        let Some(row) = row else {
            consume_password_work(password.to_owned()).await?;
            return Ok(None);
        };
        let password_hash: Option<String> = row.try_get("password_hash")?;
        let Some(password_hash) = password_hash else {
            consume_password_work(password.to_owned()).await?;
            return Ok(None);
        };
        if !verify_password(password.to_owned(), password_hash).await? {
            return Ok(None);
        }
        Ok(Some(user_from_row(&row)?))
    }

    pub async fn create_session(&self, user_id: &str, session: &Session) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)",
        )
        .bind(encode_token_hash(&session.token_hash))
        .bind(user_id)
        .bind(system_time_to_unix(session.expires_at)?)
        .execute(self.database.pool())
        .await?;
        Ok(())
    }

    pub async fn user_for_session(&self, token_hash: &TokenHash) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT users.id, users.tenant_id, users.email, users.role \
             FROM user_sessions \
             JOIN users ON users.id = user_sessions.user_id \
             WHERE user_sessions.token_hash = $1 AND user_sessions.expires_at > $2",
        )
        .bind(encode_token_hash(token_hash))
        .bind(i64::try_from(now_unix_secs())?)
        .fetch_optional(self.database.pool())
        .await?;
        row.map(|row| user_from_row(&row)).transpose()
    }

    pub async fn delete_session(&self, token_hash: &TokenHash) -> Result<()> {
        sqlx::query("DELETE FROM user_sessions WHERE token_hash = $1")
            .bind(encode_token_hash(token_hash))
            .execute(self.database.pool())
            .await?;
        Ok(())
    }

    pub async fn revoke_sessions(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(self.database.pool())
            .await?;
        Ok(())
    }

    pub async fn update_profile(
        &self,
        user_id: &str,
        email: &str,
        password: Option<&str>,
    ) -> Result<()> {
        let email = normalize_email(email)?;
        let password_hash = match password.filter(|password| !password.is_empty()) {
            Some(password) => {
                validate_password(password)?;
                Some(hash_password(password.to_owned()).await?)
            }
            None => None,
        };
        sqlx::query(
            "UPDATE users SET \
                email = $1, \
                password_hash = COALESCE($2, password_hash) \
             WHERE id = $3",
        )
        .bind(email)
        .bind(password_hash)
        .bind(user_id)
        .execute(self.database.pool())
        .await?;
        Ok(())
    }

    pub async fn reset_stream_key(&self, tenant_id: &TenantId) -> Result<String> {
        let tenant = self
            .tenants
            .find(tenant_id)
            .await?
            .context("Tenant does not exist")?;
        let stream_key = generate_secure_token()?;
        self.tenants
            .save(TenantDefinition {
                id: &tenant.id,
                name: &tenant.name,
                stream_key: &stream_key,
                active: tenant.active,
                max_concurrent_streams: tenant.max_concurrent_streams,
                notifications: &tenant.notifications,
                chat: &tenant.chat,
                overlay: &tenant.overlay,
                targets: &tenant.targets,
            })
            .await?;
        Ok(stream_key)
    }

    pub async fn find_or_create_oauth_user(
        &self,
        provider: &str,
        subject: &str,
        email: &str,
    ) -> Result<User> {
        let row = sqlx::query(
            "SELECT users.id, users.tenant_id, users.email, users.role \
             FROM user_identities \
             JOIN users ON users.id = user_identities.user_id \
             WHERE user_identities.provider = $1 AND user_identities.subject = $2",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(self.database.pool())
        .await?;
        if let Some(row) = row {
            return user_from_row(&row);
        }

        let email = normalize_email(email)?;
        let existing = sqlx::query("SELECT id, tenant_id, email, role FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(self.database.pool())
            .await?;
        let user = match existing {
            Some(row) => user_from_row(&row)?,
            None => {
                let tenant_id = TenantId::new(generate_secure_token()?)?;
                let stream_key = generate_secure_token()?;
                self.tenants
                    .save(TenantDefinition {
                        id: &tenant_id,
                        name: &email,
                        stream_key: &stream_key,
                        active: true,
                        max_concurrent_streams: 1,
                        notifications: &NotificationSettings::default(),
                        chat: &crate::config::ChatSettings::default(),
                        overlay: &crate::config::OverlaySettings::default(),
                        targets: &[],
                    })
                    .await?;
                let user = User {
                    id: generate_secure_token()?,
                    tenant_id,
                    email,
                    role: Role::User,
                };
                sqlx::query(
                    "INSERT INTO users (id, tenant_id, email, password_hash, role, created_at) \
                     VALUES ($1, $2, $3, NULL, $4, $5)",
                )
                .bind(&user.id)
                .bind(user.tenant_id.as_str())
                .bind(&user.email)
                .bind(user.role.as_str())
                .bind(i64::try_from(now_unix_secs())?)
                .execute(self.database.pool())
                .await?;
                user
            }
        };
        sqlx::query(
            "INSERT INTO user_identities (id, user_id, provider, subject) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(generate_secure_token()?)
        .bind(&user.id)
        .bind(provider)
        .bind(subject)
        .execute(self.database.pool())
        .await?;
        Ok(user)
    }

    pub async fn begin_oauth(&self, provider: &str, verifier: &str) -> Result<String> {
        let state = generate_secure_token()?;
        sqlx::query(
            "INSERT INTO oauth_attempts (state_digest, provider, pkce_verifier, expires_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(stream_key_digest(&state))
        .bind(provider)
        .bind(crate::crypto::encrypt(verifier)?)
        .bind(i64::try_from(now_unix_secs() + 600)?)
        .execute(self.database.pool())
        .await?;
        Ok(state)
    }

    pub async fn consume_oauth(&self, provider: &str, state: &str) -> Result<Option<String>> {
        let mut transaction = self.database.pool().begin().await?;
        let verifier: Option<String> = sqlx::query_scalar(
            "SELECT pkce_verifier FROM oauth_attempts \
             WHERE state_digest = $1 AND provider = $2 AND expires_at > $3",
        )
        .bind(stream_key_digest(state))
        .bind(provider)
        .bind(i64::try_from(now_unix_secs())?)
        .fetch_optional(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM oauth_attempts WHERE state_digest = $1")
            .bind(stream_key_digest(state))
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        verifier
            .map(|value| crate::crypto::decrypt(&value))
            .transpose()
    }
}

fn user_from_row(row: &sqlx::any::AnyRow) -> Result<User> {
    Ok(User {
        id: row.try_get("id")?,
        tenant_id: TenantId::new(row.try_get::<String, _>("tenant_id")?)?,
        email: row.try_get("email")?,
        role: Role::parse(row.try_get("role")?)?,
    })
}

fn normalize_email(email: &str) -> Result<String> {
    let email = email.trim().to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        bail!("Enter a valid email address");
    };
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || email.len() > 254
        || email.chars().any(char::is_whitespace)
    {
        bail!("Enter a valid email address");
    }
    Ok(email)
}

fn validate_password(password: &str) -> Result<()> {
    if password.len() < 12 || password.len() > 1024 {
        bail!("Password must contain between 12 and 1024 characters");
    }
    Ok(())
}

async fn hash_password(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let mut salt = [0_u8; 16];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| anyhow::anyhow!("Failed to generate password salt"))?;
        let salt = SaltString::encode_b64(&salt)?;
        Ok(Argon2::default()
            .hash_password(password.as_bytes(), &salt)?
            .to_string())
    })
    .await?
}

async fn verify_password(password: String, hash: String) -> Result<bool> {
    tokio::task::spawn_blocking(move || {
        let hash = PasswordHash::new(&hash)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok())
    })
    .await?
}

async fn consume_password_work(password: String) -> Result<()> {
    let _ = hash_password(password).await?;
    Ok(())
}

fn encode_token_hash(hash: &TokenHash) -> String {
    URL_SAFE_NO_PAD.encode(**hash)
}

fn system_time_to_unix(time: SystemTime) -> Result<i64> {
    Ok(i64::try_from(time.duration_since(UNIX_EPOCH)?.as_secs())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_email_addresses() {
        assert_eq!(
            normalize_email(" USER@Example.COM ").unwrap(),
            "user@example.com"
        );
        assert!(normalize_email("not-an-email").is_err());
    }

    #[tokio::test]
    async fn hashes_and_verifies_passwords_with_argon2id() {
        let hash = hash_password("correct horse battery staple".to_owned())
            .await
            .unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            verify_password("correct horse battery staple".to_owned(), hash.clone())
                .await
                .unwrap()
        );
        assert!(!hash.contains("correct horse"));
    }
}
