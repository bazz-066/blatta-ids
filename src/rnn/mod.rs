use anyhow::Result;
use ndarray::prelude::*;
use ndarray::{Axis, concatenate, stack, Array2};
use std::convert::{TryFrom, TryInto};
use std::fs;
use std::io::Read;
use tch::{nn, nn::Module, nn::OptimizerConfig, Device, nn::RNNConfig, Tensor, nn::RNN, kind::Kind};


use super::stream;

const IMAGE_DIM: i64 = 784;
const HIDDEN_NODES: i64 = 128;
const LABELS: i64 = 10;

pub enum RecurrentLayer {
    Lstm,
    Gru,
}

pub struct NetworkConfig {
   seq_len: i64,
   embedding_dim: i64,
   hidden_dim: i64,
   dropout: f64,
   layer: RecurrentLayer
}

impl NetworkConfig {
    pub fn new(seq_len: i64, 
               embedding_dim: i64,
               hidden_dim: i64,
               dropout: f64,
               layer: RecurrentLayer) -> NetworkConfig {
        NetworkConfig{
            seq_len,
            embedding_dim,
            hidden_dim,
            dropout,
            layer,
        }
    }
}

fn save(reconstructed_packets: stream::ReconstructedPackets, path: String) -> std::io::Result<()> {
    let init_filename = format!("{}/{}", path, reconstructed_packets.get_tcp_tuple()).to_string();
    fs::write(init_filename, reconstructed_packets.get_init_tcp_message())?;
    let resp_filename = format!("{}/{}", path, reconstructed_packets.get_rev_tcp_tuple()).to_string();
    fs::write(resp_filename, reconstructed_packets.get_resp_tcp_message())?;

    Ok(())
}

fn load_file(init_filename: String, resp_filename: String) -> std::io::Result<(Vec<u8>,Vec<u8>)> {
    let init_msg = fs::read(init_filename)?;

    let resp_msg = fs::read(resp_filename)?;

    Ok((init_msg, resp_msg))
}

pub fn preprocessing(message: Vec<u8>, n: usize, stride: usize) -> Array2<i64> {
    let nrows = message.len() - stride * n;
    let mut byte_sequences = Array2::<i64>::default((nrows, n+1)); 
    let mut i = 0;
    while i < (message.len() - n) {
        let j = i + n + 1;
        for index in 0..(n+1) {
            byte_sequences[[i, index]] = i64::try_from(message[i+index]).unwrap();
        }
        i+=stride;
    }

    println!("{:?}", byte_sequences);
    byte_sequences
}

//pub fn training() -> model/none {
//}

//pub fn detection() {
//}

pub fn run(config: NetworkConfig, rnn_config: RNNConfig) -> Result<()> {
    let device = Device::Cuda(0);
    println!("Is cuda: {}", device.is_cuda());
    let x = Tensor::of_slice(&vec![99, 95, 96, 93,99]).reshape(&[1,5]);
    println!("{:?}", x);
    let y = Tensor::of_slice(&vec![98i64]).reshape(&[1]);
    println!("{:?}", Vec::<i64>::from(&y));
    let vs = nn::VarStore::new(device);

    let embedding_layer = nn::embedding(
        &vs.root(),
        256,
        config.embedding_dim,
        Default::default(),
    );

    let rnn_layer = nn::lstm(
        &vs.root(),
        config.embedding_dim,
        config.hidden_dim,
        rnn_config,
    );

    let dense_layer = nn::linear(&vs.root(), config.hidden_dim, 256, Default::default());

    let mut opt = nn::Adam::default().build(&vs, 1e-3)?;
    for epoch in 1..10 {
        let embed_out = embedding_layer.forward(&x.to(device));
        println!("{:?}", embed_out.size());
        let (rnn_out, _) = rnn_layer.seq(&embed_out);
        println!("{:?}", rnn_out.size());
        let logits = dense_layer.forward(&rnn_out)
            .narrow(1, 4, 1)
            .reshape(&[1,256]);
        println!("{:?}", logits.size());
        //println!("{:?}", Vec::<f64>::from(&logits));
        let loss = logits.cross_entropy_for_logits(&y.to(device));
        opt.backward_step(&loss);
        //let test_accuracy = net
        //    .forward(&m.test_images.to(device))
        //    .accuracy_for_logits(&m.test_labels.to(device));
        println!(
            "epoch: {:4} train loss: {:8.5}",
            epoch,
            f64::from(&loss),
        );
    }
    Ok(())
}
