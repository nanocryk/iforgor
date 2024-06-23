use anyhow::anyhow;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

#[derive(Debug)]
pub struct OnDisk<T> {
    inner: T,
    path: PathBuf,
}

impl<T: Serialize + DeserializeOwned + Default> OnDisk<T> {
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        let mut file = File::open(&path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let inner = toml::from_str(&content)?;

        Ok(Self { path, inner })
    }

    pub fn open_or_default(path: PathBuf) -> anyhow::Result<Self> {
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    inner: Default::default(),
                })
            }
            Err(e) => Err(e)?,
        };

        let mut content = String::new();
        file.read_to_string(&mut content)?;

        if content.trim().is_empty() {
            return Ok(Self {
                path,
                inner: Default::default(),
            });
        }

        let inner = toml::from_str(&content)?;

        Ok(Self { path, inner })
    }

    pub fn new_from_default(path: PathBuf) -> Self {
        Self {
            path,
            inner: Default::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let folder = self
            .path
            .parent()
            .ok_or(anyhow!("expected file to be in a folder"))?;
        std::fs::create_dir_all(&folder)?;

        let mut file = File::create(&self.path)?;
        let content = toml::to_string(&self.inner)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Deref for OnDisk<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for OnDisk<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
