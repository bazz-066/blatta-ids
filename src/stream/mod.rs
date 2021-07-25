use std::collections::HashMap;
use std::thread::JoinHandle;
use std::thread;
use std::any::Any;
use std::str;
use std::os::unix::io::AsRawFd;
use std::time::Instant;
use std::sync::{mpsc, Arc, Mutex};

use smoltcp::phy::wait as phy_wait;
use smoltcp::phy::{Device, RawSocket, RxToken};
use smoltcp::time::Instant as SmoltcpInstant;
use smoltcp::wire::*;

struct WorkerHandler {
    handle: JoinHandle<Result<(), ()>>,
    sender: mpsc::Sender<Vec<u8>>
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

                    //println!("{}:{} -> {}:{}", _ipv4_packet.src_addr(), _tcp_segment.src_port(), _ipv4_packet.dst_addr(), _tcp_segment.dst_port());
                    if self.conn_list.contains_key(&key) {
                        let worker_handler = self.conn_list.get_mut(&key).unwrap();
                        worker_handler.sender.send(_frame_payload.to_vec()).unwrap();
                    }
                    else if self.conn_list.contains_key(&reverse_key) {
                        let worker_handler = self.conn_list.get_mut(&reverse_key).unwrap();
                        worker_handler.sender.send(_frame_payload.to_vec()).unwrap();
                    }
                    else {
                        let (sender, receiver) = mpsc::channel();
                        let receiver = Arc::new(Mutex::new(receiver));
                        let key_copy = key.clone();
                        let mut monitor = Monitor::new(key, reverse_key, _frame_payload.to_vec(), receiver);

                        let handle = thread::spawn(move || {
                            while monitor.start_time.elapsed().as_secs() < 10 {
                                let packet = monitor.receiver.lock().unwrap().recv().unwrap();
                                //println!("Received segment");
                                monitor.push(packet);
                            }
                            println!("Thread finished. {} secs elapsed", monitor.start_time.elapsed().as_secs());
                            // TODO: send message to main thread to remove the thread from the list
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

struct Monitor {
    key: String,
    reverse_key: String,
    init_packets: Vec<Vec<u8>>,
    resp_packets: Vec<Vec<u8>>,
    start_time: Instant,
    receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>
}

impl Monitor {
    fn new(key: String, reverse_key: String, packet: Vec<u8>, receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>) -> Monitor {
        let mut packets = Vec::new();
        packets.push(packet);
        Monitor { 
            key: key,
            reverse_key: reverse_key,
            init_packets: packets,
            resp_packets: Vec::new(),
            start_time: Instant::now(),
            receiver: receiver
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
                    break
                }
                else if _stored_segment.seq_number().ge(&_tcp_segment.seq_number()) {
                    self.init_packets.insert(i+1, packet);
                    break;
                }
            }
        }
        else {
            if self.resp_packets.len() == 0 {
                self.resp_packets.push(packet);
            }
            else {
                for i in (0..self.resp_packets.len()).rev() {
                    let _stored_packet = Ipv4Packet::new_unchecked(self.resp_packets[i].as_slice());
                    let _stored_packet_payload = _stored_packet.payload();
                    let _stored_segment = TcpPacket::new_unchecked(_stored_packet_payload);

                    // duplicate packet
                    if _stored_segment.seq_number().eq(&_tcp_segment.seq_number()) {
                        break
                    }
                    else if _stored_segment.seq_number().ge(&_tcp_segment.seq_number()) {
                        self.resp_packets.insert(i+1, packet);
                        break;
                    }
                }
            }
        }
    }
}
