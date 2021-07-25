use smoltcp::phy::wait as phy_wait;
use smoltcp::phy::{Device, RawSocket, RxToken};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetFrame, PrettyPrinter, Ipv4Packet, EthernetProtocol, IpProtocol, TcpPacket};
use std::env;
use std::os::unix::io::AsRawFd;
use std::str;
use std::thread;

mod stream;

fn main() {
    let ifname = env::args().nth(1).unwrap();
    //let mut socket = RawSocket::new(ifname.as_ref()).unwrap();
    let port_filter = Box::new([0u8]);
    let mut reader_thread = stream::StreamReaderThread::new(port_filter, false, ifname);
    reader_thread.start_listening()

    //ctrlc::set_handler(move || {
    //    println!("received Ctrl+C!");
    //    reader_thread.stop_thread(&reader_handle);
    //})
    //.expect("Error setting Ctrl-C handler");
}
