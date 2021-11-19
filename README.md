# Blatta IDS: Memory-Safe and Intelligent Network-Based HTTP Threat Detection

# What is Blatta IDS

Blatta IDS is a network-based HTTP intrusion detection built with Rust such that it is safer from memory-based exploitation and can handle multithreading better than Python-based IDS. Other features of Blatta IDS are:

1. Intelligent detection with Recurrent Neural Network (currently, only LSTM is supported)
2. Saving network payload to files for further analysis
3. Anomaly-based, hence it does not need malicious samples for training the model

Blatta IDS is developed based on this research of RNN-OD[1] and not to be confused with the other research[2] as they share the same name. Both research employ RNN but they have different ways of processing the input.

# Installation

1. Install Rust and its dependency
2. Clone the repository
```bash
	$ git clone https://github.com/bazz-066/blatta-ids
```
3. Configure the path to store the payload data. Replace the value of `path` in the `src/main.rs` file with the directory name of your own choice.
4. [Optional] Configure the threshold by changing the `threshold` value in the `src/main.rd` file.
5. Build the app
```bash
	$ cd blatta-ids/
	$ cargo build
```
6. Run the app
```bash
	$ sudo cargo run <interface_name>
```

# References

1. Pratomo, B. (2020). Low-rate attack detection with intelligent fine-grained network analysis (Doctoral dissertation, Cardiff University).
2. Pratomo, B. A., Burnap, P., & Theodorakopoulos, G. (2020). Blatta: early exploit detection on network traffic with recurrent neural networks. Security and Communication Networks, 2020.
