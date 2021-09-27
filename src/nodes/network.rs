//! Very, very WIP
//! "Delayed lockstep" networking implementation - first step towards GGPO

use macroquad::experimental::scene::{self, Handle, Node, NodeWith, RefMut};

use crate::{
    capabilities::NetworkReplicate,
    input::{self, Input, InputScheme},
    nodes::Player,
};

use std::sync::mpsc;

use nanoserde::{DeBin, SerBin};

pub trait Socket: Send {
    fn send(&self, _: &[u8]) -> Option<usize>;
    fn recv(&self, buf: &mut [u8]) -> Option<usize>;
    fn try_clone(&self) -> Option<Box<dyn Socket>>;
}

impl Socket for std::net::UdpSocket {
    fn send(&self, buf: &[u8]) -> Option<usize> {
        std::net::UdpSocket::send(self, buf).ok()
    }

    fn recv(&self, buf: &mut [u8]) -> Option<usize> {
        std::net::UdpSocket::recv(self, buf).ok()
    }

    fn try_clone(&self) -> Option<Box<dyn Socket>> {
        std::net::UdpSocket::try_clone(self)
            .ok()
            .map(|socket| Box::new(socket) as Box<dyn Socket>)
    }
}

#[cfg(feature = "steamworks")]
pub mod steam {
    use steamworks::{Client, Networking, SendType, SteamId};

    pub struct SteamSocket {
        pub client: Client,
        pub networking: Networking<steamworks::ClientManager>,
        pub opponent_id: SteamId,
    }
    unsafe impl Send for SteamSocket {}

    impl super::Socket for SteamSocket {
        fn send(&self, buf: &[u8]) -> Option<usize> {
            if self
                .networking
                .send_p2p_packet(self.opponent_id, SendType::Unreliable, buf)
            {
                return Some(buf.len());
            }

            None
        }

        fn recv(&self, buf: &mut [u8]) -> Option<usize> {
            if self.networking.is_p2p_packet_available().is_some() {
                return self.networking.read_p2p_packet(buf).map(|(_, count)| count);
            }
            None
        }

        fn try_clone(&self) -> Option<Box<dyn super::Socket>> {
            let steam_socket = SteamSocket {
                client: self.client.clone(),
                networking: self.client.networking(),
                opponent_id: self.opponent_id.clone(),
            };
            Some(Box::new(steam_socket) as Box<dyn super::Socket>)
        }
    }
}
#[derive(Debug, DeBin, SerBin)]
pub enum Message {
    /// Empty message, used for connection test
    Idle,
    RelayRequestId,
    RelayIdAssigned(u64),
    RelayConnectTo(u64),
    RelayConnected,
    Input {
        // current simulation frame
        frame: u64,
        input: Input,
    },
    Ack {
        frame: u64,
    },
}

pub struct Network {
    input_scheme: InputScheme,

    player1: Handle<Player>,
    player2: Handle<Player>,

    frame: u64,

    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,

    self_id: usize,
    // all the inputs from the beginning of the game
    // will optimize memory later
    frames_buffer: Vec<[Option<Input>; 2]>,
    acked_frames: Vec<bool>,
}

// // get a bitmask of received remote inputs out of frames_buffer
// fn remote_inputs_ack(remote_player_id: usize, buffer: &[[Option<Input>; 2]]) -> u8 {
//     let mut ack = 0;

//     #[allow(clippy::needless_range_loop)]
//     for i in 0..Network::CONSTANT_DELAY as usize {
//         if buffer[i][remote_player_id].is_some() {
//             ack |= 1 << i;
//         }
//     }
//     ack
// }

impl Network {
    /// 8-bit bitmask is used for ACK, to make CONSTANT_DELAY more than 8
    /// bitmask type should be changed
    const CONSTANT_DELAY: usize = 8;

    pub fn new(
        id: usize,
        socket: Box<dyn Socket>,
        input_scheme: InputScheme,
        player1: Handle<Player>,
        player2: Handle<Player>,
    ) -> Network {
        let (tx, rx) = mpsc::channel::<Message>();

        let (tx1, rx1) = mpsc::channel::<Message>();

        {
            let socket = socket.try_clone().unwrap();
            std::thread::spawn(move || {
                let socket = socket;
                loop {
                    let mut data = [0; 256];
                    if let Some(count) = socket.recv(&mut data) {
                        let message = DeBin::deserialize_bin(&data[0..count]).unwrap();

                        tx1.send(message).unwrap();
                    }
                }
            });
        }

        std::thread::spawn(move || {
            loop {
                if let Ok(message) = rx.recv() {
                    let data = SerBin::serialize_bin(&message);

                    let socket = socket.try_clone().unwrap();

                    // std::thread::spawn(move || {
                    //     std::thread::sleep(std::time::Duration::from_millis(
                    //         macroquad::rand::gen_range(100, 350),
                    //     ));
                    //     if macroquad::rand::gen_range(0, 100) > 90 {
                    //         let _ = socket.send(&data);
                    //     }
                    // });
                    socket.send(&data);
                }
            }
        });

        let mut frames_buffer = vec![];
        let mut acked_frames = vec![];

        // Fill first CONSTANT_DELAY frames
        // this will not really change anything - the fish will just always spend
        // first CONSTANT_DELAY frames doing nothing, not a big deal
        // But with pre-filled buffer we can avoid any special-case logic
        // at the start of the game and later on will just wait for remote
        // fish to fill up their part of the buffer
        #[allow(clippy::needless_range_loop)]
        for _ in 0..Self::CONSTANT_DELAY {
            let mut frame = [None; 2];
            frame[id as usize] = Some(Input::default());

            frames_buffer.push(frame);
            acked_frames.push(false);
        }

        Network {
            self_id: id,
            input_scheme,
            player1,
            player2,
            frame: Self::CONSTANT_DELAY as u64,
            tx,
            rx: rx1,
            frames_buffer,
            acked_frames,
        }
    }
}

impl Node for Network {
    fn fixed_update(mut node: RefMut<Self>) {
        let node = &mut *node;

        let own_input = input::collect_input(node.input_scheme);

        // Right now there are only two players, so it is possible to find out
        // remote fish id as "not ours" id. With more fish it will be more complicated
        // and ID will be part of a protocol
        let remote_id = if node.self_id == 1 { 0 } else { 1 };

        // Receive other fish input
        while let Ok(message) = node.rx.try_recv() {
            match message {
                Message::Input { frame, input } => {
                    if frame >= node.frames_buffer.len() as _ {
                        node.frames_buffer.resize(frame as usize + 1, [None, None]);
                        node.acked_frames.resize(frame as usize + 1, false);
                    }

                    node.frames_buffer[frame as usize][remote_id] = Some(input);
                    node.tx.send(Message::Ack { frame }).unwrap();
                }
                Message::Ack { frame } => {
                    node.acked_frames[frame as usize] = true;
                }
                _ => {}
            }
        }

        // re-send frames missing on remote fish
        for i in
            (node.frame as i64 - Self::CONSTANT_DELAY as i64 * 2).max(0) as u64 as u64..node.frame
        {
            if !node.acked_frames[i as usize] {
                node.tx
                    .send(Message::Input {
                        frame: i,
                        input: node.frames_buffer[i as usize][node.self_id].unwrap(),
                    })
                    .unwrap();
            }
        }

        // we just received only CONSTANT_DELAY frames, assuming we certainly
        // had remote input for all the previous frames
        // lets double check this assumption
        if node.frame > Self::CONSTANT_DELAY as _ {
            for i in 0..node.frame - Self::CONSTANT_DELAY as u64 - 1 {
                assert!(node.frames_buffer[i as usize][remote_id].is_some());
            }
        }

        // we have an input for "-CONSTANT_DELAY" frame, so we can
        // advance the simulation
        if let [Some(p1_input), Some(p2_input)] =
            node.frames_buffer[node.frame as usize - Self::CONSTANT_DELAY]
        {
            scene::get_node(node.player1).apply_input(p1_input);
            scene::get_node(node.player2).apply_input(p2_input);

            // advance the simulation
            for NodeWith { node, capability } in scene::find_nodes_with::<NetworkReplicate>() {
                (capability.network_update)(node);
            }

            if node.frame >= node.frames_buffer.len() as _ {
                node.frames_buffer
                    .resize(node.frame as usize + 1, [None, None]);
                node.acked_frames.resize(node.frame as usize + 1, false);
            }
            node.frames_buffer[node.frame as usize][node.self_id] = Some(own_input);
            node.frame += 1;
        }
    }
}
