use std::rc::Rc;

struct FishLibrary {
    lib: libloading::Library,
}

impl FishLibrary {
    fn new() -> FishLibrary {
        let lib = unsafe { libloading::Library::new("libfishsteam_sys.so").unwrap() };

        FishLibrary { lib }
    }

    fn steam_connection_new(&self) -> *mut () {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<unsafe extern "C" fn() -> *mut ()>>(
                    b"steam_connection_new",
                )
                .unwrap())()
        }
    }

    fn steam_connection_update(&self, connection: *mut ()) {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<unsafe extern "C" fn(_: *mut ())>>(
                    b"steam_connection_update",
                )
                .unwrap())(connection)
        }
    }

    fn steam_connection_status(&self, connection: *mut ()) -> SteamStatus {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<unsafe extern "C" fn(_: *mut ()) -> SteamStatus>>(
                    b"steam_connection_status",
                )
                .unwrap())(connection)
        }
    }

    fn steam_connection_self_id(&self, connection: *mut ()) -> u64 {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<unsafe extern "C" fn(_: *mut ()) -> u64>>(
                    b"steam_connection_self_id",
                )
                .unwrap())(connection)
        }
    }

    fn steam_connection_opponent_id(&self, connection: *mut ()) -> u64 {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<unsafe extern "C" fn(_: *mut ()) -> u64>>(
                    b"steam_connection_opponent_id",
                )
                .unwrap())(connection)
        }
    }

    fn steam_connection_send(
        &self,
        connection: *mut (),
        remote: u64,
        bytes: *const u8,
        bytes_len: usize,
    ) -> i64 {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<
                    unsafe extern "C" fn(_: *mut (), _: u64, _: *const u8, _: usize) -> i64,
                >>(b"steam_connection_send")
                .unwrap())(connection, remote, bytes, bytes_len)
        }
    }

    fn steam_connection_try_recv(
        &self,
        connection: *mut (),
        bytes: *mut u8,
        bytes_len: usize,
    ) -> i64 {
        unsafe {
            (self
                .lib
                .get::<libloading::Symbol<
                    unsafe extern "C" fn(_: *mut (), _: *mut u8, _: usize) -> i64,
                >>(b"steam_connection_try_recv")
                .unwrap())(connection, bytes, bytes_len)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SteamId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LobbyId(pub(crate) u64);

#[repr(C)]
#[derive(Debug)]
pub enum Error {
    SteamError,
    WrongLobby,
    NoOpponent,
    CreateLobbyFailed,
}

#[repr(C)]
#[derive(Debug)]
pub enum SteamStatus {
    WaitingForLobbies,
    CreatingLobby,
    WaitingForConnection(u64),
    Connecting,
    Error(Error),
    WaitingForProbe,
    Ready,
}

/// Incapsulate everything about steam users, friends etc
/// Allowing to match players into game, the only thing FishFight is intrested in
#[derive(Clone)]
pub struct Steam {
    library: Rc<FishLibrary>,
    connection: *mut (),
}

impl Steam {
    pub fn new() -> Result<Steam, Error> {
        let library = FishLibrary::new();

        let connection = library.steam_connection_new();
        if connection.is_null() {
            return Err(Error::SteamError);
        }

        Ok(Steam {
            library: Rc::new(library),
            connection,
        })
    }

    pub fn update(&mut self) {
        self.library.steam_connection_update(self.connection);
    }

    pub fn opponent_id(&self) -> Option<SteamId> {
        let opponent_id = self.library.steam_connection_opponent_id(self.connection);
        match opponent_id {
            0 => None,
            id => Some(SteamId(id)),
        }
    }

    pub fn self_id(&self) -> SteamId {
        let self_id = self.library.steam_connection_self_id(self.connection);
        SteamId(self_id)
    }

    pub fn status(&self) -> SteamStatus {
        self.library.steam_connection_status(self.connection)
    }

    pub fn send(&self, remote: SteamId, bytes: &[u8]) -> Option<usize> {
        let res = self.library.steam_connection_send(
            self.connection,
            remote.0,
            bytes.as_ptr(),
            bytes.len(),
        );
        if res == -1 {
            None
        } else {
            Some(res as usize)
        }
    }

    pub fn try_recv(&self, bytes: &mut [u8]) -> Option<usize> {
        let res = self.library.steam_connection_try_recv(
            self.connection,
            bytes.as_mut_ptr(),
            bytes.len(),
        );
        if res == -1 {
            None
        } else {
            Some(res as usize)
        }
    }
}
