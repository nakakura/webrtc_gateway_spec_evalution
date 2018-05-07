#![feature(drain_filter)]
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate bincode;
extern crate rustc_serialize;
extern crate chrono;

mod udp;
mod rtp;

use chrono::*;
use bincode::{serialize, deserialize, Bounded, ErrorKind};
use tokio_core::reactor::Core;
use futures::*;
use futures::sync::mpsc;

use std::collections::HashMap;
use std::thread;
use std::net::SocketAddr;

fn main() {
    let rtp_recv_addr: SocketAddr = format!("0.0.0.0:{}", 10001).parse().unwrap();
    let udp_recv_addr: SocketAddr = format!("0.0.0.0:{}", 10002).parse().unwrap();

    let (recv_rtp_tx, recv_rtp_rx) = mpsc::channel::<Vec<u8>>(5000);
    let (recv_udp_tx, recv_udp_rx) = mpsc::channel::<Vec<u8>>(5000);

    let th_rtp_recv = udp::receiver(rtp_recv_addr, recv_rtp_tx);
    let th_udp_recv = udp::receiver(udp_recv_addr, recv_udp_tx);

    let recv_udp_rx = recv_udp_rx.map(binary_to_ts);

    let recv_rtp_rx = recv_rtp_rx.map(|mut recv| {
        let len = recv.len() - recv[recv.len() - 1] as usize;
        let mut padding = recv.split_off(len);
        let len = padding.len();
        let plen = padding.split_off(len - 1);
        padding
    }).map(binary_to_ts);

    let recv = recv_udp_rx.select(recv_rtp_rx);

    let th_relay = thread::spawn(|| {
        let limit = Bounded(1500);
        let mut core = Core::new().unwrap();
        let mut time_hash = HashMap::new();
        let r = recv.fold(time_hash, |mut sum: HashMap<u64, (bool, DateTime<Utc>)>, x| {
            if sum.contains_key(&x.0) {
                let val = sum.get(&x.0).unwrap();
                let duration = if val.0 {
                    Utc::now().signed_duration_since(val.1)
                } else {
                    val.1.signed_duration_since(Utc::now())
                }.num_nanoseconds().unwrap() as f64;
                println!("{:?} mill sec faster than rtp", duration / 1000000f64);
            } else {
                println!("flag {}", x.1);
                sum.insert(x.0, (x.1, Utc::now()));
            }
            Ok(sum)
        });
        let _ = core.run(r);
    });

    let _ = th_rtp_recv.join();
    let _ = th_relay.join();
}

type SenderTuple = (mpsc::Sender<Vec<u8>>, mpsc::Sender<Vec<u8>>, mpsc::Sender<Vec<u8>>);

fn binary_to_ts(binary: Vec<u8>) -> (u64, bool) {
    let (counter, flag): (u64, bool) = deserialize(&binary).unwrap();
    (counter, flag)
}

fn broadcast(sum: SenderTuple, acc: Vec<u8>) -> Result<SenderTuple, ()> {
    let s1 = sum.0.send(acc.clone()).wait().unwrap();
    let s2 = sum.1.send(acc.clone()).wait().unwrap();
    let s3 = sum.2.send(acc).wait().unwrap();
    Ok((s1, s2, s3))
}

