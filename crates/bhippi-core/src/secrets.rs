use bhippi_types::{BhippiError, Result};

pub trait SecretStore: Send + Sync {
    fn set(&self, name: &str, secret: &str) -> Result<()>;
    fn get(&self, name: &str) -> Result<Option<String>>;
    fn delete(&self, name: &str) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct OsKeychain {
    service: String,
}

impl Default for OsKeychain {
    fn default() -> Self {
        Self::new("bhippi")
    }
}

impl OsKeychain {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        if name.is_empty() || name.chars().any(char::is_control) {
            return Err(secret_error(
                "secret name is empty or contains control characters",
                "Use a stable provider or deployment credential name.",
            ));
        }
        keyring::Entry::new(&self.service, name).map_err(|error| {
            secret_error(
                format!("cannot open the OS credential entry: {error}"),
                "Unlock the operating-system credential store and retry.",
            )
        })
    }
}

impl SecretStore for OsKeychain {
    fn set(&self, name: &str, secret: &str) -> Result<()> {
        self.entry(name)?.set_password(secret).map_err(|error| {
            secret_error(
                format!("cannot save the credential: {error}"),
                "Unlock the operating-system credential store and retry.",
            )
        })
    }

    fn get(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(secret_error(
                format!("cannot read the credential: {error}"),
                "Unlock the operating-system credential store and retry.",
            )),
        }
    }

    fn delete(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(secret_error(
                format!("cannot delete the credential: {error}"),
                "Unlock the operating-system credential store and retry.",
            )),
        }
    }
}

fn secret_error(reason: impl Into<String>, hint: impl Into<String>) -> BhippiError {
    BhippiError::Secret {
        reason: reason.into(),
        hint: Some(hint.into()),
    }
}
