use engine::BtConfig;
use server::{BacktestResponse, ZileanServer};
use uuid::timestamp::context;
use zilean::*;
use zmq::Context;
fn main() {
    let zilean_server = ZileanServer::new();
    let context = Context::new();
    let responder = context.socket(zmq::REP).unwrap();
    responder
        .bind("ipc:///tmp/zilean_backtest.ipc")
        .expect("Failed to bind socket");

    loop {
        let message = responder
            .recv_string(0)
            .expect("Failed to receive message")
            .unwrap();

        if message.starts_with("LAUNCH_BACKTEST") {
            let config: BtConfig = serde_json::from_str(&message["LAUNCH_BACKTEST".len()..])
                .expect("Failed to parse config");
            let backtest_id = zilean_server.launch_backtest(config);
            let response = serde_json::to_string(&BacktestResponse {
                backtest_id,
                status: String::from("Backtest successfully launched"),
                message: "".to_string(),
            });
        } else if message.starts_with("GET_NEXT_TICK") {
            let backtest_id = &message["GET_NEXT_TICK".len()..].trim();
            if let Some(tick) = zilean_server.get_next_tick(backtest_id) {
                let tick_response =
                    serde_json::to_string(&tick).expect("Failed to serialize tick data");
                responder
                    .send(tick_response, 0)
                    .expect("Failed to send tick data");
            } else {
                responder
                    .send("Tick not found", 0)
                    .expect("Failed to send error message");
            }
        } else {
            responder
                .send("Unknown command", 0)
                .expect("Failed to send unknown command response");
        }
    }
}
