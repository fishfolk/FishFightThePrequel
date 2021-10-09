use macroquad::{
    experimental::collections::storage,
    prelude::*,
    ui::{self, hash, root_ui, widgets},
};

use crate::{gui::GuiResources, input::InputScheme, nodes::network::Message, GameType};

use std::net::UdpSocket;

const WINDOW_WIDTH: f32 = 700.;
const WINDOW_HEIGHT: f32 = 400.;

fn local_game_ui(ui: &mut ui::Ui, players: &mut Vec<InputScheme>) -> Option<GameType> {
    let gui_resources = storage::get_mut::<GuiResources>();

    if players.len() < 2 {
        if is_key_pressed(KeyCode::V) {
            //
            if !players.contains(&InputScheme::KeyboardLeft) {
                players.push(InputScheme::KeyboardLeft);
            }
        }
        if is_key_pressed(KeyCode::L) {
            //
            if !players.contains(&InputScheme::KeyboardRight) {
                players.push(InputScheme::KeyboardRight);
            }
        }
        for ix in 0..quad_gamepad::MAX_DEVICES {
            let state = gui_resources.gamepads.state(ix);

            if state.digital_state[quad_gamepad::GamepadButton::Start as usize] {
                //
                if !players.contains(&InputScheme::Gamepad(ix)) {
                    players.push(InputScheme::Gamepad(ix));
                }
            }
        }
    }

    ui.label(None, "To connect:");
    ui.label(None, "Press Start on gamepad");
    ui.separator();

    ui.label(None, "Or V for keyboard 1");
    ui.label(None, "Or L for keyboard 2");

    ui.separator();
    ui.separator();
    ui.separator();
    ui.separator();

    ui.group(hash!(), vec2(WINDOW_WIDTH / 2. - 50., 70.), |ui| {
        if players.get(0).is_none() {
            ui.label(None, "Player 1: Not connected");
        }
        if let Some(input) = players.get(0) {
            ui.label(None, "Player 1: Connected!");
            ui.label(None, &format!("{:?}", input));
        }
    });
    ui.group(hash!(), vec2(WINDOW_WIDTH / 2. - 50., 70.), |ui| {
        if players.get(1).is_none() {
            ui.label(None, "Player 2: Not connected");
        }
        if let Some(input) = players.get(1) {
            ui.label(None, "Player 2: Connected!");
            ui.label(None, &format!("{:?}", input));
        }
    });
    if players.len() == 2 {
        let btn_a = is_gamepad_btn_pressed(&*gui_resources, quad_gamepad::GamepadButton::A);
        let enter = is_key_pressed(KeyCode::Enter);

        if ui.button(None, "Ready! (A) (Enter)") || btn_a || enter {
            return Some(GameType::Local(players.clone()));
        }
    }

    None
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum ConnectionKind {
    Lan,
    #[cfg(feature = "steamworks")]
    Steam,
}

#[derive(Debug, PartialEq)]
enum ConnectionStatus {
    Unknown,
    Connected,
}

struct LanConnection {
    socket: UdpSocket,
    local_addr: String,
    opponent_addr: String,
    status: ConnectionStatus,
}

impl LanConnection {
    fn new() -> LanConnection {
        use std::net::SocketAddr;

        let addrs = [
            SocketAddr::from(([0, 0, 0, 0], 3400)),
            SocketAddr::from(([0, 0, 0, 0], 3401)),
            SocketAddr::from(([0, 0, 0, 0], 3402)),
            SocketAddr::from(([0, 0, 0, 0], 3403)),
        ];

        let socket = UdpSocket::bind(&addrs[..]).unwrap();

        let local_addr = format!("{}", socket.local_addr().unwrap());
        socket.set_nonblocking(true).unwrap();

        LanConnection {
            socket,
            local_addr,
            opponent_addr: "".to_string(),
            status: ConnectionStatus::Unknown,
        }
    }

    fn update(&mut self) {
        let mut buf = [0; 100];
        if self.socket.recv(&mut buf).is_ok() {
            let _message: Message = nanoserde::DeBin::deserialize_bin(&buf[..]).ok().unwrap();
            self.status = ConnectionStatus::Connected;
        }
    }

    pub fn probe(&mut self) -> Option<()> {
        self.socket.connect(&self.opponent_addr).ok()?;

        for _ in 0..100 {
            self.socket
                .send(&nanoserde::SerBin::serialize_bin(&Message::Idle))
                .ok()?;
        }

        None
    }
}

#[cfg(feature = "steamworks")]
mod steam {
    use std::sync::{Arc, Mutex};
    use steamworks::{
        CallbackHandle, ChatMemberStateChange, Client, LobbyChatUpdate, LobbyId, Matchmaking,
        P2PSessionConnectFail, P2PSessionRequest, SingleClient, SteamId,
    };

    #[derive(Debug)]
    pub enum Error {
        SteamError(steamworks::SteamError),
        WrongLobby,
        NoOpponent,
        CreateLobbyFailed,
    }

    impl From<steamworks::SteamError> for Error {
        fn from(error: steamworks::SteamError) -> Error {
            Error::SteamError(error)
        }
    }

    #[derive(Debug)]
    pub enum SteamStatus {
        WaitingForLobbies,
        CreatingLobby,
        WaitingForConnection(LobbyId),
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
                                self.status = SteamStatus::WaitingForConnection(*lobby);
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
                    let message = &nanoserde::SerBin::serialize_bin(&super::Message::Idle);
                    self.client.networking().send_p2p_packet(
                        self.opponent_id.unwrap(),
                        steamworks::SendType::Unreliable,
                        message,
                    );
                }
                SteamStatus::Ready => {}
                SteamStatus::Error(_) => {}
            }
        }
    }
}

struct NetworkUiState {
    input_scheme: InputScheme,
    connection_kind: ConnectionKind,
    lan_connection: Option<LanConnection>,
    #[cfg(feature = "steamworks")]
    steam_connection: Option<Result<steam::SteamConnection, steam::Error>>,
}

fn is_gamepad_btn_pressed(gui_resources: &GuiResources, btn: quad_gamepad::GamepadButton) -> bool {
    for ix in 0..quad_gamepad::MAX_DEVICES {
        let state = gui_resources.gamepads.state(ix);
        if state.digital_state[btn as usize] && !state.digital_state_prev[btn as usize] {
            return true;
        }
    }

    false
}

fn network_game_ui(ui: &mut ui::Ui, state: &mut NetworkUiState) -> Option<GameType> {
    let mut connection_kind_ui = state.connection_kind as usize;

    #[cfg(not(feature = "steamworks"))]
    let options = &["Lan network"];
    #[cfg(feature = "steamworks")]
    let options = &["Lan network", "Steam"];

    widgets::ComboBox::new(hash!(), options)
        .ratio(0.4)
        .label("LanConnection type")
        .ui(ui, &mut connection_kind_ui);

    match connection_kind_ui {
        x if x == ConnectionKind::Lan as usize => {
            state.connection_kind = ConnectionKind::Lan;
        }
        #[cfg(feature = "steamworks")]
        x if x == ConnectionKind::Steam as usize => {
            state.connection_kind = ConnectionKind::Steam;
        }
        _ => unreachable!(),
    }

    if state.connection_kind == ConnectionKind::Lan {
        if state.lan_connection.is_none() {
            state.lan_connection = Some(LanConnection::new());
        }
        let connection = state.lan_connection.as_mut().unwrap();
        let mut self_addr = connection.local_addr.clone();

        widgets::InputText::new(hash!())
            .ratio(0.4)
            .label("Self addr")
            .ui(ui, &mut self_addr);

        widgets::InputText::new(hash!())
            .ratio(0.4)
            .label("Opponent addr")
            .ui(ui, &mut connection.opponent_addr);

        connection.update();

        if ui.button(None, "Probe connection") {
            connection.probe();
        }

        ui.label(
            None,
            &format!("LanConnection status: {:?}", connection.status),
        );

        if connection.status == ConnectionStatus::Connected
            && ui.button(None, "Connect (A) (Enter)")
        {
            return Some(GameType::Network {
                socket: Box::new(connection.socket.try_clone().unwrap()),
                id: if connection.local_addr > connection.opponent_addr {
                    0
                } else {
                    1
                },
                input_scheme: state.input_scheme,
            });
        }
    }

    #[cfg(feature = "steamworks")]
    if state.connection_kind == ConnectionKind::Steam {
        if state.steam_connection.is_none() {
            state.steam_connection = Some(steam::SteamConnection::new());
        }
        let connection = state.steam_connection.as_mut().unwrap();
        match connection {
            Err(err) => {
                ui.label(None, &format!("Error: {:?}", err));
                if ui.button(None, "Try again") {
                    state.steam_connection = None;
                }
            }
            Ok(connection) => {
                connection.update();

                ui.label(None, &format!("Status: {:?}", connection.status));

                if let steam::SteamStatus::Ready = connection.status {
                    if ui.button(None, "Connect") {
                        use crate::nodes::network::steam::SteamSocket;

                        let opponent_id = connection.opponent_id.unwrap();
                        return Some(GameType::Network {
                            socket: Box::new(SteamSocket {
                                client: connection.client.clone(),
                                networking: connection.client.networking(),
                                opponent_id,
                            }),
                            id: if connection.self_id > opponent_id {
                                0
                            } else {
                                1
                            },
                            input_scheme: state.input_scheme,
                        });
                    }
                }
            }
        }
    }

    ui.label(
        vec2(430., 310.),
        &format!("Input: {:?}", state.input_scheme),
    );
    ui.label(vec2(360., 330.), "Press V/L/Start to change");
    if is_key_pressed(KeyCode::V) {
        state.input_scheme = InputScheme::KeyboardLeft;
    }
    if is_key_pressed(KeyCode::L) {
        state.input_scheme = InputScheme::KeyboardRight;
    }
    for ix in 0..quad_gamepad::MAX_DEVICES {
        let gui_resources = storage::get_mut::<GuiResources>();
        let gamepad_state = gui_resources.gamepads.state(ix);

        if gamepad_state.digital_state[quad_gamepad::GamepadButton::Start as usize] {
            state.input_scheme = InputScheme::Gamepad(ix);
        }
    }

    None
}

pub async fn game_type() -> GameType {
    let mut players = vec![];

    let mut network_ui_state = NetworkUiState {
        lan_connection: None,
        #[cfg(feature = "steamworks")]
        steam_connection: None,
        input_scheme: InputScheme::KeyboardLeft,
        connection_kind: ConnectionKind::Lan,
    };

    let mut tab = 0;
    loop {
        let mut res = None;

        {
            let mut gui_resources = storage::get_mut::<GuiResources>();

            gui_resources.gamepads.update();

            if is_key_pressed(KeyCode::Left)
                || is_gamepad_btn_pressed(&*gui_resources, quad_gamepad::GamepadButton::BumperLeft)
                || is_gamepad_btn_pressed(&*gui_resources, quad_gamepad::GamepadButton::ThumbLeft)
            {
                tab += 1;
                tab %= 2;
            }
            // for two tabs going left and right is the same thing
            if is_key_pressed(KeyCode::Right)
                || is_gamepad_btn_pressed(&*gui_resources, quad_gamepad::GamepadButton::BumperRight)
                || is_gamepad_btn_pressed(&*gui_resources, quad_gamepad::GamepadButton::ThumbRight)
            {
                tab += 1;
                tab %= 2;
            }
        }

        {
            let gui_resources = storage::get_mut::<GuiResources>();
            root_ui().push_skin(&gui_resources.skins.login_skin);
        }

        root_ui().window(
            hash!(),
            Vec2::new(
                screen_width() / 2. - WINDOW_WIDTH / 2.,
                screen_height() / 2. - WINDOW_HEIGHT / 2.,
            ),
            Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            |ui| match widgets::Tabbar::new(
                hash!(),
                vec2(WINDOW_WIDTH - 50., 50.),
                &["<< Local game, LT", "Network game, RT >>"],
            )
            .selected_tab(Some(&mut tab))
            .ui(ui)
            {
                0 => {
                    res = local_game_ui(ui, &mut players);
                }
                1 => {
                    res = network_game_ui(ui, &mut network_ui_state);
                }
                _ => unreachable!(),
            },
        );

        root_ui().pop_skin();

        if let Some(res) = res {
            return res;
        }
        next_frame().await;
    }
}

pub async fn location_select() -> String {
    let mut hovered: i32 = 0;

    let mut old_mouse_position = mouse_position();

    // skip a frame to let Enter be unpressed from the previous screen
    next_frame().await;

    let mut prev_up = false;
    let mut prev_down = false;
    let mut prev_right = false;
    let mut prev_left = false;

    loop {
        let mut gui_resources = storage::get_mut::<GuiResources>();

        gui_resources.gamepads.update();

        let mut up = is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W);
        let mut down = is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S);
        let mut right = is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D);
        let mut left = is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A);
        let mut start = is_key_pressed(KeyCode::Enter);

        for ix in 0..quad_gamepad::MAX_DEVICES {
            use quad_gamepad::GamepadButton::*;

            let state = gui_resources.gamepads.state(ix);
            if state.status == quad_gamepad::ControllerStatus::Connected {
                up |= !prev_up && state.analog_state[1] < -0.5;
                down |= !prev_down && state.analog_state[1] > 0.5;
                left |= !prev_left && state.analog_state[0] < -0.5;
                right |= !prev_right && state.analog_state[0] > 0.5;
                start |= (state.digital_state[A as usize] && !state.digital_state_prev[A as usize])
                    || (state.digital_state[Start as usize]
                        && !state.digital_state_prev[Start as usize]);

                prev_up = state.analog_state[1] < -0.5;
                prev_down = state.analog_state[1] > 0.5;
                prev_left = state.analog_state[0] < -0.5;
                prev_right = state.analog_state[0] > 0.5;
            }
        }
        clear_background(BLACK);

        let levels_amount = gui_resources.levels.len();

        root_ui().push_skin(&gui_resources.skins.main_menu_skin);

        let rows = (levels_amount + 2) / 3;
        let w = (screen_width() - 120.) / 3. - 50.;
        let h = (screen_height() - 180.) / rows as f32 - 50.;

        {
            if up {
                hovered -= 3;
                let ceiled_levels_amount = levels_amount as i32 + 3 - (levels_amount % 3) as i32;
                if hovered < 0 {
                    hovered = (hovered + ceiled_levels_amount as i32) % ceiled_levels_amount;
                    if hovered >= levels_amount as i32 {
                        hovered -= 3;
                    }
                }
            }

            if down {
                hovered += 3;
                if hovered >= levels_amount as i32 {
                    let row = hovered % 3;
                    hovered = row;
                }
            }
            if left {
                hovered -= 1;
            }
            if right {
                hovered += 1;
            }
            hovered = (hovered + levels_amount as i32) % levels_amount as i32;

            let levels = &mut gui_resources.levels;

            for (n, level) in levels.iter_mut().enumerate() {
                let is_hovered = hovered == n as i32;

                let rect = Rect::new(
                    60. + (n % 3) as f32 * (w + 50.) - level.size * 30.,
                    90. + 25. + (n / 3) as f32 * (h + 50.) - level.size * 30.,
                    w + level.size * 60.,
                    h + level.size * 60.,
                );
                if old_mouse_position != mouse_position() && rect.contains(mouse_position().into())
                {
                    hovered = n as _;
                }

                if is_hovered {
                    level.size = level.size * 0.8 + 1.0 * 0.2;
                } else {
                    level.size = level.size * 0.9 + 0.0;
                }

                if ui::widgets::Button::new(level.preview)
                    .size(rect.size())
                    .position(rect.point())
                    .ui(&mut *root_ui())
                    || start
                {
                    root_ui().pop_skin();
                    let level = &levels[hovered as usize];
                    return level.map.clone();
                }
            }
        }

        root_ui().pop_skin();

        old_mouse_position = mouse_position();

        next_frame().await;
    }
}
