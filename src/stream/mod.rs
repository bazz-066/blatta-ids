use std::collections::HashMap;
use std::thread::JoinHandle;
use std::thread;
use std::any::Any;
use std::str;
use std::os::unix::io::AsRawFd;
use std::time::{Instant, Duration};
use std::sync::{mpsc, Arc, Mutex};
use std::sync::mpsc::SendError;

use smoltcp::phy::wait as phy_wait;
use smoltcp::phy::{Device, RawSocket, RxToken};
use smoltcp::socket::TcpState;
use smoltcp::time::Instant as SmoltcpInstant;
use smoltcp::wire::*;

struct WorkerHandler {
    handle: JoinHandle<Result<(), ()>>,     //the reconstructing thread handle
    sender: mpsc::Sender<Vec<u8>>,          //to send the packet to be reconstructed
}

impl WorkerHandler {
    fn new(handle: JoinHandle<Result<(), ()>>, sender: mpsc::Sender<Vec<u8>>) -> WorkerHandler {
        WorkerHandler { handle, sender }
    }
}

pub struct StreamReaderThread {
    conn_list: HashMap<String, WorkerHandler>,
    ready_conn_list: HashMap<String, WorkerHandler>,
    port_filter: Box<[u8]>,
    is_delete_read_conn: bool,
    ifname: String,
    //socket: RawSocket
}

impl StreamReaderThread {
    pub fn new(port_filter: Box<[u8]>, is_delete_read_conn: bool, ifname: String) -> StreamReaderThread {
        StreamReaderThread {
            conn_list: HashMap::new(),
            ready_conn_list: HashMap::new(),
            port_filter: port_filter,
            is_delete_read_conn: is_delete_read_conn,
            ifname: ifname,
        }
    }

    pub fn start_listening(&mut self) -> () {
        let mut socket = RawSocket::new(self.ifname.as_ref()).unwrap();
        
        // channels declaration
        let (packets_sender, packets_receiver) = mpsc::channel();
        let (req_cmd_sender, req_cmd_receiver) = mpsc::channel();
        let (resp_cmd_sender, resp_cmd_receiver) = mpsc::channel();

        // set up mutex for the receivers
        let packets_receiver = Arc::new(Mutex::new(packets_receiver));
        let req_cmd_receiver = Arc::new(Mutex::new(req_cmd_receiver));
        let resp_cmd_receiver = Arc::new(Mutex::new(resp_cmd_receiver));

        let mut rst_object = ReadyServeThread::new(req_cmd_receiver, resp_cmd_sender, packets_receiver);
        let _rst_handle = thread::spawn(move || {
            loop {
                let data_received = rst_object.req_cmd_receiver.lock().unwrap().try_recv();
                match data_received {
                    Ok(cmd) => match cmd {
                        Message::ReadyConnRequest => {
                            let retval = rst_object.pop_conn();
                            if retval == true {
                                println!("New ready connection has been sent");
                            }
                        },
                        Message::StopThread => break
                    }, // TODO: process command
                    Err(why) => {}
                }

                let data_received = rst_object.packets_receiver.lock().unwrap().try_recv();
                match data_received {
                    Ok(reconstructed_packets) => {
                        println!("New ready TCP connection!");
                        rst_object.push_conn(reconstructed_packets); 
                    }
                    Err(why) => {}
                }

                // sleep?
                thread::sleep(Duration::from_millis(100));
            }
        });

        loop {
            phy_wait(socket.as_raw_fd(), None).unwrap();
            let (rx_token, _) = socket.receive().unwrap();
            rx_token.consume(SmoltcpInstant::now(), |buffer| {
                let _frame = EthernetFrame::new_unchecked(&buffer);
                let _frame_payload = _frame.payload();
                let _ipv4_packet = Ipv4Packet::new_unchecked(&_frame_payload);
                
                if _ipv4_packet.protocol() == IpProtocol::Tcp {
                    let _packet_payload = _ipv4_packet.payload();
                    let _tcp_segment = TcpPacket::new_unchecked(&_packet_payload);
                    let key = format!("{}:{}:{}:{}", _ipv4_packet.src_addr(), _tcp_segment.src_port(), _ipv4_packet.dst_addr(), _tcp_segment.dst_port());
                    let reverse_key = format!("{}:{}:{}:{}", _ipv4_packet.dst_addr(), _tcp_segment.dst_port(), _ipv4_packet.src_addr(), _tcp_segment.src_port());
                    let mut is_processed = false;

                    //println!("{}:{} -> {}:{}", _ipv4_packet.src_addr(), _tcp_segment.src_port(), _ipv4_packet.dst_addr(), _tcp_segment.dst_port());
                    if self.conn_list.contains_key(&key) {
                        let worker_handler = self.conn_list.get_mut(&key).unwrap();
                        match worker_handler.sender.send(_frame_payload.to_vec()) {
                            Ok(_) => {
                                is_processed = true;
                            }
                            Err(why) => {
                                println!("Removing thread {}", key);
                                self.conn_list.remove(&key);
                            }
                        }
                    }
                    else if self.conn_list.contains_key(&reverse_key) {
                        let worker_handler = self.conn_list.get_mut(&reverse_key).unwrap();
                        match worker_handler.sender.send(_frame_payload.to_vec()) {
                            Ok(()) => {
                                is_processed = true;
                            }
                            Err(why) => {
                                println!("Removing thread {}", reverse_key);
                                self.conn_list.remove(&reverse_key);
                            }
                        }
                    }
                    
                    if is_processed == false {
                        let (sender, receiver) = mpsc::channel();
                        let receiver = Arc::new(Mutex::new(receiver));
                        let key_copy = key.clone();
                        let cloned_packets_sender = packets_sender.clone();
                        let mut monitor = Monitor::new(key, reverse_key, _frame_payload.to_vec(), receiver, cloned_packets_sender);

                        let handle = thread::spawn(move || {
                            while monitor.last_update.elapsed().as_secs() < 10 {
                                let packet = monitor.receiver.lock().unwrap().recv().unwrap();
                                //println!("Received segment");
                                monitor.push(packet);
                                if monitor.tcp_state == TcpState::Closed {
                                    break
                                }
                            }
                            println!("Thread finished. {} secs elapsed", monitor.start_time.elapsed().as_secs());
                            // TODO: send message to main thread to remove the thread from the list
                            monitor.send_reconstructed_packets();
                            Ok(())
                        });

                        sender.send(_frame_payload.to_vec()).unwrap();
                        let worker_handler = WorkerHandler::new(handle, sender);
                        self.conn_list.insert(key_copy, worker_handler);
                    }

                }
                Ok(())
            }).unwrap();
        }
    }

    //pub fn stop_thread<T>(&self, handle: &JoinHandle<T>) {
    //    self.is_done = true;
    //    println!("Waiting for thread to finish...");
    //    handle.join().unwrap();
    //}
}

pub enum Message{
    ReadyConnRequest,
    StopThread
}

pub struct ReconstructedPackets {
    init_packets: Vec<Vec<u8>>,
    resp_packets: Vec<Vec<u8>>
}

impl ReconstructedPackets {
    pub fn new(init_packets: Vec<Vec<u8>>, resp_packets: Vec<Vec<u8>>) -> ReconstructedPackets {
        ReconstructedPackets {
            init_packets: init_packets,
            resp_packets: resp_packets
        }
    }
}

pub struct ReadyServeThread {
    ready_conns: Vec<ReconstructedPackets>,
    req_cmd_receiver: Arc<Mutex<mpsc::Receiver<Message>>>,
    resp_cmd_sender: mpsc::Sender<ReconstructedPackets>,
    packets_receiver: Arc<Mutex<mpsc::Receiver<ReconstructedPackets>>>
}

impl ReadyServeThread {
    pub fn new(req_cmd_receiver: Arc<Mutex<mpsc::Receiver<Message>>>, resp_cmd_sender: mpsc::Sender<ReconstructedPackets>, packets_receiver: Arc<Mutex<mpsc::Receiver<ReconstructedPackets>>>) -> ReadyServeThread {
        ReadyServeThread {
            ready_conns: Vec::new(),
            req_cmd_receiver: req_cmd_receiver,
            packets_receiver: packets_receiver,
            resp_cmd_sender: resp_cmd_sender
        }
    }

    pub fn push_conn(&mut self, conn: ReconstructedPackets) {
        self.ready_conns.push(conn);
    }

    pub fn pop_conn(&mut self) -> bool {
        if !self.ready_conns.is_empty() {
            let ready_conn = self.ready_conns.remove(0);
            self.resp_cmd_sender.send(ready_conn).unwrap();
            true
        }
        else {
            false
        }
    }
}

struct Monitor {
    key: String,
    reverse_key: String,
    init_packets: Vec<Vec<u8>>,
    resp_packets: Vec<Vec<u8>>,
    start_time: Instant,
    last_update: Instant,
    receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
    packets_sender: mpsc::Sender<ReconstructedPackets>,
    tcp_state: TcpState
}

impl Monitor {
    fn new(key: String, reverse_key: String, packet: Vec<u8>, receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>, packets_sender: mpsc::Sender<ReconstructedPackets>) -> Monitor {
        let mut packets = Vec::new();
        packets.push(packet);
        Monitor { 
            key: key,
            reverse_key: reverse_key,
            init_packets: packets,
            resp_packets: Vec::new(),
            start_time: Instant::now(),
            last_update: Instant::now(),
            receiver: receiver,
            packets_sender: packets_sender,
            tcp_state: TcpState::Established,
        }
    }

    fn push(&mut self, packet: Vec<u8>) {
        let cloned_packet = packet.clone();
        let _ipv4_packet = Ipv4Packet::new_unchecked(cloned_packet.as_slice());
        let _ipv4_payload = _ipv4_packet.payload();
        let _tcp_segment = TcpPacket::new_unchecked(_ipv4_payload);

        let key = format!("{}:{}:{}:{}", _ipv4_packet.src_addr(), _tcp_segment.src_port(), _ipv4_packet.dst_addr(), _tcp_segment.dst_port());
        //println!("{}", key);

        if key == self.key {
            // convert to TcpPacket
            // look for the appropriate place to put the segment
            for i in (0..self.init_packets.len()).rev() {
                let _stored_packet = Ipv4Packet::new_unchecked(self.init_packets[i].as_slice());
                let _stored_packet_payload = _stored_packet.payload();
                let _stored_segment = TcpPacket::new_unchecked(_stored_packet_payload);

                // duplicate packet
                if _stored_segment.seq_number().eq(&_tcp_segment.seq_number()) {
                    self.last_update = Instant::now();
                    break
                }
                else if _stored_segment.seq_number().ge(&_tcp_segment.seq_number()) {
                    self.init_packets.insert(i+1, packet);
                    self.last_update = Instant::now();
                    break;
                }
            }
        }
        else {
            if self.resp_packets.len() == 0 {
                self.last_update = Instant::now();
                self.resp_packets.push(packet);
            }
            else {
                for i in (0..self.resp_packets.len()).rev() {
                    let _stored_packet = Ipv4Packet::new_unchecked(self.resp_packets[i].as_slice());
                    let _stored_packet_payload = _stored_packet.payload();
                    let _stored_segment = TcpPacket::new_unchecked(_stored_packet_payload);

                    // duplicate packet
                    if _stored_segment.seq_number().eq(&_tcp_segment.seq_number()) {
                        self.last_update = Instant::now();
                        break
                    }
                    else if _stored_segment.seq_number().ge(&_tcp_segment.seq_number()) {
                        self.resp_packets.insert(i+1, packet);
                        self.last_update = Instant::now();
                        break;
                    }
                }
            }
        }

        if _tcp_segment.fin() || self.tcp_state != TcpState::Established {
            match self.tcp_state {
                TcpState::Established => self.tcp_state = TcpState::FinWait1,
                TcpState::FinWait1 => self.tcp_state = TcpState::FinWait2,
                TcpState::FinWait2 => self.tcp_state = TcpState::TimeWait,
                _ => {}
            }
        }

        if _tcp_segment.ack() && self.tcp_state == TcpState::TimeWait {
            println!("TCP Connection ended normally");
            self.tcp_state = TcpState::Closed;
        }
    }

    fn send_reconstructed_packets(&mut self) {
        let init_packets = self.init_packets.drain(0..).collect();
        let resp_packets = self.resp_packets.drain(0..).collect();
        let reconstructed_packets = ReconstructedPackets::new(init_packets, resp_packets);

        self.packets_sender.send(reconstructed_packets).unwrap();
    }
}
