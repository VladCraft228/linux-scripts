use crate::profiles::Profile;
use std::{
    env,
    fs::{self, DirBuilder, File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        raw::c_int,
        unix::{
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
            net::UnixDatagram,
        },
    },
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

// Linux flock uses the lifetime of the open file description as the lock lifetime.
extern "C" {
    fn flock(fd: c_int, operation: c_int) -> c_int;
}

const LOCK_EX: c_int = 2;
const LOCK_NB: c_int = 4;
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Request {
    Activate = 1,
    EnableTray = 2,
    Background = 3,
    IndicatorSettings = 4,
    Automatic = 5,
    Balanced = 6,
    Performance = 7,
    PowerSaver = 8,
    Quit = 9,
}

impl Request {
    pub fn from_mode_id(id: &str) -> Option<Self> {
        match id {
            "automatic" => Some(Self::Automatic),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            "power-saver" => Some(Self::PowerSaver),
            _ => None,
        }
    }

    pub fn mode(self) -> Option<Profile> {
        match self {
            Self::Automatic => Some(Profile::Automatic),
            Self::Balanced => Some(Profile::Balanced),
            Self::Performance => Some(Profile::Performance),
            Self::PowerSaver => Some(Profile::PowerSaver),
            _ => None,
        }
    }

    fn decode(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Activate),
            2 => Some(Self::EnableTray),
            3 => Some(Self::Background),
            4 => Some(Self::IndicatorSettings),
            5 => Some(Self::Automatic),
            6 => Some(Self::Balanced),
            7 => Some(Self::Performance),
            8 => Some(Self::PowerSaver),
            9 => Some(Self::Quit),
            _ => None,
        }
    }
}

pub struct Instance {
    _lock: File,
    socket_path: PathBuf,
    socket: Option<UnixDatagram>,
}

impl Instance {
    pub fn acquire(request: Request) -> io::Result<Option<Self>> {
        let uid = fs::metadata("/proc/self")?.uid();
        let path = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/tmp").join(format!("vatra-{uid}")));
        let directory = path.join("vatra-instance");
        if !path.exists() && path.parent().is_some_and(|parent| parent == std::path::Path::new("/tmp")) {
            create_private_dir(&path, uid)?;
        }
        verify_private_dir(&path, uid)?;
        create_private_dir(&directory, uid)?;

        let lock = OpenOptions::new()
            .write(true).create(true).mode(0o600).open(directory.join("instance.lock"))?;
        let socket_path = directory.join("activate.sock");
        let deadline = Instant::now() + Duration::from_secs(2);
        let client = UnixDatagram::unbound()?;
        loop {
            // The file stays open in Instance until the process stops listening.
            if unsafe { flock(lock.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
                // A quit request never starts a new instance.
                if matches!(request, Request::Quit) { return Ok(None); }
                match fs::remove_file(&socket_path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                let socket = UnixDatagram::bind(&socket_path)?;
                return Ok(Some(Self { _lock: lock, socket_path, socket: Some(socket) }));
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::WouldBlock {
                return Err(error);
            }
            match client.send_to(&[request as u8], &socket_path) {
                Ok(_) => return Ok(None),
                Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused)
                    && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn listen(&mut self, mut activate: impl FnMut(Request) -> bool + Send + 'static) -> io::Result<()> {
        let socket = self.socket.take().expect("instance listener starts once");
        thread::Builder::new().name("vatra-activation".into()).spawn(move || {
            let mut message = [0; 2];
            while let Ok(size) = socket.recv(&mut message) {
                if size != 1 { continue; }
                let Some(request) = Request::decode(message[0]) else { continue };
                if !activate(request) { break; }
            }
        })?;
        Ok(())
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}

fn create_private_dir(path: &std::path::Path, uid: u32) -> io::Result<()> {
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    verify_private_dir(path, uid)
}

fn verify_private_dir(path: &std::path::Path, uid: u32) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Vatra needs a private runtime directory"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_modes_round_trip_through_single_instance_protocol() {
        for mode in Profile::QUICK {
            let request = Request::from_mode_id(mode.id()).unwrap();
            assert_eq!(Request::decode(request as u8).and_then(Request::mode), Some(mode));
        }
        assert!(Request::from_mode_id("custom").is_none());
        assert!(Request::decode(255).is_none());
        assert!(matches!(Request::decode(4), Some(Request::IndicatorSettings)));
        assert!(matches!(Request::decode(9), Some(Request::Quit)));
    }
}
