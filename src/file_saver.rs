use anyhow::Result;
use log::debug;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub trait FileSaver {
    fn save(
        &self,
        path: PathBuf,
        content: Vec<u8>,
        executable: bool,
        overwrite: bool,
    ) -> Result<()>;
}

pub struct FileSavers;

impl FileSavers {
    pub fn new() -> Box<dyn FileSaver> {
        #[cfg(unix)]
        return Box::new(UnixFileSaver {});
        #[cfg(windows)]
        return Box::new(WindowsFileSaver {});
    }
}

#[cfg(unix)]
struct UnixFileSaver {}

#[cfg(unix)]
impl FileSaver for UnixFileSaver {
    fn save(
        &self,
        path: PathBuf,
        content: Vec<u8>,
        executable: bool,
        overwrite: bool,
    ) -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;

        let mode = if executable { 0o755 } else { 0o644 };
        let mut file = if overwrite {
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .mode(mode)
                .open(&path)?
        } else {
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(mode)
                .open(&path)?
        };

        debug!("Saving file {:?}, size: {:?}", path, content.len());

        file.write_all(&content)?;

        debug!("Successfully saved file: {:?}", path);

        Ok(())
    }
}

#[cfg(windows)]
struct WindowsFileSaver {}

#[cfg(windows)]
impl FileSaver for WindowsFileSaver {
    fn save(
        &self,
        path: PathBuf,
        content: Vec<u8>,
        executable: bool,
        overwrite: bool,
    ) -> Result<()> {
        let mut file = if overwrite {
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?
        } else {
            OpenOptions::new().write(true).truncate(true).open(&path)?
        };

        debug!("Saving file {:?}, size: {:?}", path, content.len());

        file.write_all(&content)?;

        debug!("Successfully saved file: {:?}", path);

        Ok(())
    }
}
