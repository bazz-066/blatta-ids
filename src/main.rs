use smoltcp::phy::wait as phy_wait;
use smoltcp::phy::{Device, RawSocket, RxToken};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetFrame, PrettyPrinter, Ipv4Packet, EthernetProtocol, IpProtocol, TcpPacket};
use std::env;
use std::os::unix::io::AsRawFd;
use std::str;
use std::sync::mpsc;
use std::thread;

use tch::nn::RNNConfig;

//use pcap_file::pcap::{PcapParser, PcapReader};
//use pcap_file::PcapError;
//use std::fs::File;

mod stream;
mod rnn;

use rnn::RecurrentModel;

fn main() {
    let ifname = env::args().nth(1).unwrap();
    let path = "/home/baskoro/Documents/Research/IDS/data/".to_string();
    //let mut socket = RawSocket::new(ifname.as_ref()).unwrap();
    let port_filter = Box::new([80u8]);
    let mut srt_controller = stream::StreamReaderController::new(port_filter, false, ifname);
    
    let (packets_sender, packets_receiver): (mpsc::Sender<stream::ReconstructedPackets>, mpsc::Receiver<stream::ReconstructedPackets>) = mpsc::channel();

    let handle = thread::spawn(move || {
        let n: usize = 5;
        let stride: usize = 1;
        let embedding_dim: i64 = 64;
        let hidden_dim: i64 = 32;
        let dropout: f64 = 0.2;

        let network_config = rnn::NetworkConfig::new(n, stride, embedding_dim, hidden_dim, dropout, rnn::RecurrentLayer::Lstm);
        let rnn_config: RNNConfig = Default::default();

        let recurrent_model = rnn::LSTMModel::new(network_config, rnn_config);

        loop {
            let data_received = srt_controller.get_ready_conn();
            //println!("Trying to get ready connection");
            match data_received {
                Some(reconstructed_packets) => {
                    
                    //println!("New TCP message: {}", &reconstructed_packets.get_tcp_tuple());
                    //println!("Init TCP message: {}", String::from_utf8_lossy(&reconstructed_packets.get_init_tcp_message()));
                    //println!("Resp TCP message: {}", String::from_utf8_lossy(&reconstructed_packets.get_resp_tcp_message()));
                    //println!("New TCP message: {:x?}", &reconstructed_packets.get_init_tcp_message());
                    
                    rnn::save(&reconstructed_packets, &path);
                    recurrent_model.train_model_with_packet(&reconstructed_packets, stream::PacketDirection::Init);
                    //let byte_sequences = rnn::preprocessing(reconstructed_packets.get_init_tcp_message(), n, stride);
                    //reconstructed_packets.get_resp_tcp_message();
                },
                None => {}
            }
        }
    });

    handle.join();
    

    //let config = rnn::NetworkConfig::new(5, 64, 32, 0.2, rnn::RecurrentLayer::Lstm);
    //let rnn_config: RNNConfig = Default::default();

    //rnn::run(config, rnn_config);
    //println!("HOOOOOUUU");
    //rnn::preprocessing((0..20).collect(), 5, 1);

    //ctrlc::set_handler(move || {
    //    println!("received Ctrl+C!");
    //    reader_thread.stop_thread(&reader_handle);
    //})
    //.expect("Error setting Ctrl-C handler");
    //

    /*
    let path = env::args().nth(1).unwrap();
    let file_in = File::open(path).expect("Error opening file");
    let pcap_reader = PcapReader::new(file_in).unwrap();
    let pcap = pcap_reader;
    // Creates a new parser and parse the pcap header
    let mut src = &pcap[..];
    let (rem, pcap_parser) = PcapParser::new(&pcap[..]).unwrap();
    src = rem;
    
    loop {
        match pcap_parser.next_packet(src) {
            Ok((rem, packet)) => {
                let frame = EthernetFrame::new_unchecked(packet.data);

                src = rem;
                if rem.is_empty() {
                    break;
                }
            },
            Err(PcapError::IncompleteBuffer(needed)) => {},
            Err(_) => {}
        }
    }*/
}
