#![feature(drain_filter)]
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate bincode;
extern crate rustc_serialize;

mod udp;
mod rtp;

use bincode::{serialize, deserialize, Bounded, ErrorKind};
use tokio_core::reactor::Core;
use futures::*;
use futures::sync::mpsc;

use std::thread;
use std::net::SocketAddr;

fn main() {
    let target_addr_str = "127.0.0.1";
    let target_addr_rtp: SocketAddr = format!("{}:{}", target_addr_str, 10001).parse().unwrap();
    let target_addr_udp: SocketAddr = format!("{}:{}", target_addr_str, 10002).parse().unwrap();

    let (send_rtp_tx, send_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let (send_udp_tx, send_udp_rx) = mpsc::channel::<Vec<u8>>(5000);

    let th_send_rtp = udp::sender(send_rtp_rx.map(move |x| (target_addr_rtp, x)));
    let th_send_udp = udp::sender(send_udp_rx.map(move |x| (target_addr_udp, x)));

    let (recv_udp_tx, recv_udp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let recv_addr: SocketAddr = format!("0.0.0.0:{}", 10000).parse().unwrap();
    let th_recv = udp::receiver(recv_addr, recv_udp_tx);

    let th_relay = thread::spawn(|| {
        let limit = Bounded(1500);
        let mut core = Core::new().unwrap();
        let r = recv_udp_rx.fold((send_rtp_tx, send_udp_tx, 0u64), |sum, mut s| {
            unsafe {
                rtp::set_padding(s.as_mut_ptr(), 1);
            };

            let rtp_flag_bin = serialize(&(sum.2, true), limit).unwrap();
            let udp_flag_bin = serialize(&(sum.2, false), limit).unwrap();
            s.extend(rtp_flag_bin.clone());
            s.extend(vec!(rtp_flag_bin.len() as u8 + 1u8));

            let s0 = sum.0.send(s).wait().unwrap();
            let s1 = sum.1.send(udp_flag_bin).wait().unwrap();
            Ok((s0, s1, sum.2 + 1))
        });
        let _ = core.run(r);
    });

    let _ = th_recv.join();
    let _ = th_send_rtp.join();
    let _ = th_send_udp.join();
    let _ = th_relay.join();
}

type SenderTuple = (mpsc::Sender<Vec<u8>>, mpsc::Sender<Vec<u8>>, mpsc::Sender<Vec<u8>>);

fn broadcast(sum: SenderTuple, acc: Vec<u8>) -> Result<SenderTuple, ()> {
    let s1 = sum.0.send(acc.clone()).wait().unwrap();
    let s2 = sum.1.send(acc.clone()).wait().unwrap();
    let s3 = sum.2.send(acc).wait().unwrap();
    Ok((s1, s2, s3))
}

