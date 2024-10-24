# Zilean Backtest Engine

Zilean is a high-performance backtesting engine designed for cryptocurrency quantitative trading strategies. It enables efficient backtesting of various trading algorithms, providing a robust environment to evaluate and improve trading strategies using real market data.

## Features
- High-speed backtesting for cryptocurrency markets
- Efficient data handling with support for ClickHouse databases
- Plug-and-play data sources, including real-time databases and local files
- Supports different order types and time-in-force policies
- Flexible latency models (fixed, random, or no latency)
- Integration with ZeroMQ for IPC communication between client and server
- Unix socket and shared memory support for optimal performance

## Project Structure
```
Zilean/
|-- core/
|   |-- dataloader.rs      # Loads market data from database or file
|   |-- engine.rs          # Core backtesting logic
|   |-- market.rs          # Data structures for market, orders, balances
|   |-- server.rs          # Zilean backtest server for handling client requests
|-- examples/              # Example usage of Zilean for testing and demonstration
|-- Cargo.toml             # Rust project configuration file
|-- README.md              # Project documentation (you're reading it!)
```

## Requirements
- Rust (latest stable version)
- Python 3 (for client scripts and testing purposes)
- ZeroMQ for inter-process communication
- ClickHouse database for storing market data

## Running the Backtest Engine
1. Start the Zilean backtest server:
   ```bash
   cargo run --release
   ```
2. Use the Python client to launch a backtest. An example script (`zileanExample.py`) is provided in the `examples/` directory. Run it using:
   ```bash
   python examples/zileanExample.py
   ```

## Usage
### Launching a Backtest
Use the Python client to send a "LAUNCH_BACKTEST" request to the server. The request includes backtest parameters such as the symbol, start and end times, exchange details, and other configuration settings.

```python
message = {
    "exchanges": ["BinanceSpot"],
    "symbol": "BTC_USDT",
    "start_time": 0,
    "end_time": 1727930047114,
    "balance": {"total": 0, "available": 0, "freezed": 0},
    "source": "Database",
    "fee_rate": {"maker_fee": 0, "taker_fee": 0},
}
```

### Monitoring Backtest Performance
After launching the backtest, you can monitor its progress by sending requests to get ticks or account information. This is done through ZeroMQ REQ/REP or PUB/SUB for real-time updates.

### Improving Performance
Zilean's architecture aims for high efficiency, leveraging:
- Shared memory access for data exchange
- Unix sockets for reduced latency in inter-process communication

### Performance Considerations
When pushing for low latency (e.g., below 10 microseconds for IPC), it's recommended to:
- Use shared memory for data exchange to minimize overhead
- Avoid unnecessary data serialization/deserialization
- Profile and optimize both server-side (Rust) and client-side (Python) implementations

## TODO
- Support futures contract backtesting
- Implement support for market orders and limit IOC (Immediate-Or-Cancel) orders
- Expand data sources and improve integration with new exchanges
- Add more advanced latency models and risk management features
- Improve the visualization of backtest results with charts and analytics
