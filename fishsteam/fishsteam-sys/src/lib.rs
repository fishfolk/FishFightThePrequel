use std::sync::{Arc, Mutex};
use steamworks::{
    CallbackHandle, ChatMemberStateChange, Client, LobbyChatUpdate, LobbyId, Matchmaking,
    P2PSessionConnectFail, P2PSessionRequest, SendType, SingleClient, SteamId,
};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum Error {
    SteamError,
    WrongLobby,
    NoOpponent,
    CreateLobbyFailed,
}

impl From<steamworks::SteamError> for Error {
    fn from(_error: steamworks::SteamError) -> Error {
        Error::SteamError
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub enum SteamStatus {
    WaitingForLobbies,
    CreatingLobby,
    WaitingForConnection(u64),
    Connecting,
    Error(Error),
    WaitingForProbe,
    Ready,
}

pub struct SteamConnection {
    pub client: Client<steamworks::ClientManager>,
    pub self_id: SteamId,
    pub opponent_id: Option<SteamId>,
    single: SingleClient<steamworks::ClientManager>,
    matchmaking: Matchmaking<steamworks::ClientManager>,
    pub status: SteamStatus,
    lobbies: Arc<Mutex<Option<Result<Vec<LobbyId>, steamworks::SteamError>>>>,
    lobby_id: Arc<Mutex<Option<Result<LobbyId, ()>>>>,
    incoming_connection: Arc<Mutex<Option<SteamId>>>,
    _callbacks: Vec<CallbackHandle<steamworks::ClientManager>>,
}

impl SteamConnection {
    pub fn new() -> Result<SteamConnection, Error> {
        let (client, single) = Client::init()?;

        let self_id = client.user().steam_id();
        println!("Self user id: {:?}", self_id);

        let mut _callbacks = vec![];
        let cb = client.register_callback({
            let client = client.clone();
            move |p: P2PSessionRequest| {
                println!("Got P2PSessionRequest callback: {:?}", p);

                let networking = client.networking();
                networking.accept_p2p_session(p.remote);
            }
        });
        _callbacks.push(cb);

        let cb = client.register_callback({
            move |p: P2PSessionConnectFail| {
                println!("Got P2PSessionConnectFail callback: {:?}", p);
            }
        });
        _callbacks.push(cb);

        let incoming_connection = Arc::new(Mutex::new(None));
        let cb = client.register_callback({
            let incoming_connection = incoming_connection.clone();

            move |p: LobbyChatUpdate| {
                println!("Got LobbyChatUpdate callback: {:?}", p);

                match p.member_state_change {
                    ChatMemberStateChange::Entered => {
                        *incoming_connection.lock().unwrap() = Some(p.user_changed);
                    }
                    _ => {}
                }
            }
        });
        _callbacks.push(cb);

        let matchmaking = client.matchmaking();

        let lobbies = Arc::new(Mutex::new(None));

        matchmaking.request_lobby_list({
            let lobbies = lobbies.clone();
            move |res| {
                *lobbies.lock().unwrap() = Some(res);
            }
        });

        Ok(SteamConnection {
            client,
            single,
            self_id,
            opponent_id: None,
            matchmaking,
            incoming_connection,
            status: SteamStatus::WaitingForLobbies,
            lobbies,
            lobby_id: Arc::new(Mutex::new(None)),
            _callbacks,
        })
    }

    pub fn update(&mut self) {
        self.single.run_callbacks();

        match self.status {
            SteamStatus::WaitingForLobbies => {
                if let Some(lobbies) = &*self.lobbies.lock().unwrap() {
                    match lobbies {
                        Ok(lobbies) if lobbies.is_empty() => {
                            self.status = SteamStatus::CreatingLobby;
                            let lobby_id = self.lobby_id.clone();
                            self.matchmaking.create_lobby(
                                steamworks::LobbyType::Public,
                                2,
                                move |id| {
                                    *lobby_id.lock().unwrap() = Some(id.map_err(|_| ()));
                                },
                            );
                        }
                        Ok(lobbies) => {
                            self.status = SteamStatus::Connecting;
                            let lobby = self.lobby_id.clone();
                            self.matchmaking.join_lobby(lobbies[0], move |res| {
                                *lobby.lock().unwrap() = Some(res);
                            });
                        }
                        Err(err) => {
                            self.status = SteamStatus::Error(err.clone().into());
                        }
                    }
                }
            }
            SteamStatus::WaitingForConnection(_) => {
                if let Some(opponent_id) = &*self.incoming_connection.lock().unwrap() {
                    self.opponent_id = Some(*opponent_id);
                    println!(
                        "Ready to connect. Self_id: {:?}, opponent_id: {:?}",
                        self.self_id,
                        self.opponent_id.unwrap()
                    );

                    self.status = SteamStatus::WaitingForProbe;
                    return;
                }
            }
            SteamStatus::CreatingLobby => {
                if let Some(lobby_id) = &*self.lobby_id.lock().unwrap() {
                    match lobby_id {
                        Err(_) => {
                            self.status = SteamStatus::Error(Error::CreateLobbyFailed);
                        }
                        Ok(lobby) => {
                            self.status = SteamStatus::WaitingForConnection(lobby.raw());
                        }
                    }
                }
            }
            SteamStatus::Connecting => {
                if let Some(Err(_)) = &*self.lobby_id.lock().unwrap() {
                    self.status = SteamStatus::Error(Error::WrongLobby);
                    return;
                }
                if let Some(Ok(lobby)) = &*self.lobby_id.lock().unwrap() {
                    let opponents = self.matchmaking.lobby_members(*lobby);

                    if opponents.len() != 2 {
                        self.status = SteamStatus::Error(Error::WrongLobby);
                        return;
                    }
                    println!("opponents: {:?}", opponents);

                    let opponent = opponents.iter().find(|opponent| **opponent != self.self_id);
                    if opponent.is_none() {
                        self.status = SteamStatus::Error(Error::NoOpponent);
                    }
                    self.opponent_id = Some(*opponent.unwrap());

                    println!(
                        "Ready to connect. Self_id: {:?}, opponent_id: {:?}, in lobby {:?}",
                        self.self_id,
                        self.opponent_id.unwrap(),
                        lobby
                    );
                    self.status = SteamStatus::WaitingForProbe;
                }
            }
            SteamStatus::WaitingForProbe => {
                if self.client.networking().is_p2p_packet_available().is_some() {
                    self.status = SteamStatus::Ready;
                }
                self.client.networking().send_p2p_packet(
                    self.opponent_id.unwrap(),
                    steamworks::SendType::Unreliable,
                    &[23],
                );
            }
            SteamStatus::Ready => {}
            SteamStatus::Error(_) => {}
        }
    }
}

#[no_mangle]
pub extern "C" fn steam_connection_new() -> *mut () {
    let steam_connection = SteamConnection::new();

    match steam_connection {
        Ok(connection) => {
            let boxed_connection = Box::new(connection);
            Box::into_raw(boxed_connection) as *mut ()
        }
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn steam_connection_update(connection: *mut ()) {
    let connection: &mut SteamConnection = unsafe { &mut *(connection as *mut _) };
    connection.update();
}

#[no_mangle]
pub extern "C" fn steam_connection_status(connection: *mut ()) -> SteamStatus {
    let connection: &mut SteamConnection = unsafe { &mut *(connection as *mut _) };
    connection.status.clone()
}

#[no_mangle]
pub extern "C" fn steam_connection_self_id(connection: *mut ()) -> u64 {
    let connection: &mut SteamConnection = unsafe { &mut *(connection as *mut _) };
    connection.self_id.raw()
}

#[no_mangle]
pub extern "C" fn steam_connection_opponent_id(connection: *mut ()) -> u64 {
    let connection: &mut SteamConnection = unsafe { &mut *(connection as *mut _) };
    connection.opponent_id.map_or(0, |id| id.raw())
}

#[no_mangle]
pub extern "C" fn steam_connection_send(
    connection: *mut (),
    remote: u64,
    bytes: *const u8,
    bytes_len: usize,
) -> i64 {
    let connection: &mut SteamConnection = unsafe { &mut *(connection as *mut _) };

    let buf = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    if connection.client.networking().send_p2p_packet(
        SteamId::from_raw(remote),
        SendType::Unreliable,
        buf,
    ) {
        return buf.len() as i64;
    }
    return -1;
}

#[no_mangle]
pub extern "C" fn steam_connection_try_recv(
    connection: *mut (),
    bytes: *mut u8,
    bytes_len: usize,
) -> i64 {
    let connection: &mut SteamConnection = unsafe { &mut *(connection as *mut _) };

    let buf = unsafe { std::slice::from_raw_parts_mut(bytes, bytes_len) };

    let networking = connection.client.networking();
    if networking.is_p2p_packet_available().is_some() {
        return networking
            .read_p2p_packet(buf)
            .map_or(-1, |(_, count)| count as i64);
    }

    return -1;
}
