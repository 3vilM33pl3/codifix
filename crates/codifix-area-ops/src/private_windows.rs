// SPDX-License-Identifier: AGPL-3.0-or-later
//! Windows bindings preserve the existing operator's protected-file contract.
use crate::{Failure, Result};
use codifix_platform_files::windows;
use std::{fs::File, path::Path};

pub struct Directory(windows::Directory);
impl Directory {
    pub fn open(path: &Path) -> Result<Self> {
        windows::Directory::open(path)
            .map(Self)
            .map_err(|_| Failure::PrivateFiles)
    }
    pub fn create(&self, name: &str) -> Result<File> {
        self.0.create(name).map_err(|_| Failure::PrivateFiles)
    }
    pub fn ensure_absent(&self, names: &[&str]) -> Result<()> {
        self.0
            .ensure_absent(names)
            .map_err(|_| Failure::PrivateFiles)
    }
    pub fn publish(&self, partial: &str, name: &str, file: &File) -> Result<()> {
        self.0
            .publish(partial, name, file)
            .map_err(|_| Failure::PrivateFiles)
    }
}

pub struct Input {
    pub file: File,
    guard: windows::Input,
}
impl Input {
    pub fn open(path: &Path, limit: u64) -> Result<Self> {
        let guard = windows::Input::open(path, limit).map_err(|_| Failure::PrivateFiles)?;
        let file = guard.file.try_clone().map_err(|_| Failure::PrivateFiles)?;
        Ok(Self { file, guard })
    }
    pub fn unchanged(&self) -> Result<()> {
        self.guard.unchanged().map_err(|_| Failure::PrivateFiles)
    }
}
