use crate::dataloader::DataSource;
use crate::{market::*, ZConfig};
use crate::round::round6;
use rand::Rng;
// use rustc_hash::FxHashMap;
use super::{dataloader::DataLoader, server::BacktestResponse};
use log::{debug, info};
use rand_distr::{Distribution, Normal};
use sonic_rs::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::i64;
use std::sync::Arc;
use tokio::sync::Mutex;


#[derive(Debug, Default, PartialEq)]
pub enum BacktestState {
    #[default]
    Ready,
    Running,
    Paused,
    Finished,
}

pub enum LatencyModel {
    None,
    Fixed(i64),
    Random(i64, i64),
    Positivedistribution(f64, f64),
}

impl LatencyModel {
    pub fn order_with_latency(&self, order: Order) -> Order {
        match self {
            LatencyModel::None => order,
            LatencyModel::Fixed(latency) => Order {
                timestamp: order.timestamp + latency,
                ..order
            },
            LatencyModel::Random(a, b) => Order {
                timestamp: order.timestamp + rand::thread_rng().gen_range(*a..*b),
                ..order
            },
            LatencyModel::Positivedistribution(mean, std) => {
                let normal = Normal::new(*mean, *std).unwrap();
                let latency = normal.sample(&mut rand::thread_rng()).max(0.0); // Ensure latency is non-negative
                Order {
                    timestamp: order.timestamp + latency as i64,
                    ..order
                }
            }
        }
    }
}

// back = backQueuePosition , front = nowQueuePosition
#[derive(Clone, PartialEq)]
pub enum FillModel {
    None,
    Random(f64, f64),
    /// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
    /// calculated as `f(back) / (f(back) + f(front))`.
    PowerProbQueueFunc(f64),
    /// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
    /// calculated as `f(back) / f(back + front)`.
    PowerProbQueueFunc2(f64),
    /// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
    /// calculated as `1 - f(front / (front + back))`.
    PowerProbQueueFunc3(f64),
    /// This probability model uses a logarithmic function `f(x) = log(1 + x)` to adjust the
    /// probability which is calculated as `f(back) / f(back + front)`.
    LogProbQueueFunc2,
    /// This probability model uses a logarithmic function `f(x) = log(1 + x)` to adjust the
    /// probability which is calculated as `f(back) / (f(back) + f(front))`.
    LogProbQueueFunc,
}

impl FillModel {
    pub fn prob(&self, back: f64, front: f64) -> f64 {
        match self {
            FillModel::None => 1.0,
            FillModel::Random(min, max) => rand::thread_rng().gen_range(*min..*max),
            FillModel::PowerProbQueueFunc(n) => {
                let back_power = back.powf(*n);
                let front_power = front.powf(*n);
                back_power / (back_power + front_power)
            }
            FillModel::PowerProbQueueFunc2(n) => back.powf(*n) / (back + front).powf(*n),
            FillModel::PowerProbQueueFunc3(n) => {
                // println!("back: {}, front: {}, prob: {}", back, front, 1.0 - (front / (front + back)).powf(*n));
                1.0 - (front / (front + back)).powf(*n)
            }
            FillModel::LogProbQueueFunc2 => {
                let back_log = (1.0 + back).ln();
                let front_back_log = (1.0 + front + back).ln();
                back_log / front_back_log
            }
            FillModel::LogProbQueueFunc => {
                let back_log = (1.0 + back).ln();
                let front_log = (1.0 + front).ln();
                back_log / (back_log + front_log)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct BtConfig {
    pub exchanges: Vec<Exchange>,
    pub symbol: String,
    // v1, do not use trade data to match
    // trade_ex: Exchange,
    pub start_time: i64,
    pub end_time: i64,
    pub source: Option<DataSource>,
    pub balance: Balance,
    pub fee_rate: FeeRate,
}

impl BtConfig {
    pub fn parse(config: &str) -> BtConfig {
        sonic_rs::from_str(config).expect("Failed to parse config")
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct OrderList {
    inner: VecDeque<Order>,
}

impl OrderList {
    pub fn insert_order(&mut self, order: Order) {
        self.inner.push_back(order);
    }

    pub fn remove_order(&mut self, cid: String) -> Option<Order> {
        if let Some(pos) = self.inner.iter().position(|x| x.cid == cid) {
            return self.inner.remove(pos);
        }
        None
    }

    pub fn execute_orders(&mut self, depth: &Depth, fill_model: FillModel) -> Vec<FilledStack> {
        // println!("{:?}", self.inner);
        let mut filled = vec![];
        self.inner.retain_mut(|order| {
            if order.state == OrderState::Canceled || order.state == OrderState::Filled {
                let threshold:i64 = 9_999_999_999_999_999; // microsecond
                let mut order_time = order.timestamp;
                let mut depth_time = depth.local_timestamp;
                if order_time > threshold { // nanosecond
                    order_time /= 1_000_000;
                    depth_time /= 1_000_000;
                }
                // keep this order for seconds after it is filled or canceled, then remove it in case of latency
                //  magic number: 3_000_000 mean 3 s
                if order_time + 3_100 < depth_time {
                    return false;
                }
                return true;
            }
            // info!("depth.timestamp: {:?}, order.timestamp: {:?}", depth.local_timestamp, order.timestamp);
            if depth.local_timestamp < order.timestamp || order.exchange != depth.exchange {
                return true; // keep this order
            }
            let (filled_price, filled_amount) = order.execute(depth, fill_model.clone());

            if filled_price != 0.0 {
                filled.push(FilledStack {
                    cid: order.cid.clone(),
                    exchange: order.exchange,
                    symbol: order.symbol.clone(),
                    contract_type: order.contract_type.clone(),
                    side: order.position_side.clone(),
                    leverage: order.leverage,
                    take_profit: order.take_profit,
                    stop_loss: order.stop_loss,
                    filled_price,
                    filled_amount,
                    open_price: order.open_price,
                    post_price: order.price,
                    freeze_margin: order.margin,
                    amount_total: order.amount,
                });
            }
            true
        });
        filled
    }
}

#[derive(Debug, Default)]
pub struct FilledStack {
    pub cid: String,
    pub exchange: Exchange,
    pub symbol: String,
    pub side: PositionSide,
    pub contract_type: ContractType,
    pub leverage: u32,
    pub take_profit: Option<f64>,
    pub stop_loss: Option<f64>,
    pub open_price: Option<f64>,
    pub filled_price: f64,
    pub filled_amount: f64,
    pub post_price: f64,
    pub freeze_margin: f64,
    pub amount_total: f64,
}

#[derive(Serialize, Default)]
pub struct TickResponseDepth {
    pub depth: Depth,
    pub account: Account,
    pub orders: OrderList,
}

#[derive(Serialize, Default)]
pub struct TickResponseTrade {
    pub trade: Trade,
    pub account: Account,
    pub orders: OrderList,
}

// v1, do not support hedge backtest
pub struct ZileanV1 {
    pub config: BtConfig,
    pub zconfig: ZConfig,
    // trade: Trade,
    order_list: OrderList,
    pub account: Account,
    data_loader: Arc<Mutex<DataLoader>>,
    data_cache: VecDeque<Depth>,
    trade_cache: VecDeque<Trade>,
    pub next_tick: String,
    latency: LatencyModel,
    fill_model: FillModel,
    state: BacktestState,
    depth: Depth,
}

impl ZileanV1 {
    pub async fn new(config: BtConfig, zconfig: ZConfig) -> ZileanV1 {
        Self {
            config: config.clone(),
            zconfig: zconfig.clone(),
            // trade: Trade::default(),
            order_list: OrderList::default(),
            account: Account::default(),
            data_loader: Arc::new(Mutex::new(DataLoader::new(50_000, &config, zconfig).await)),
            data_cache: VecDeque::new(),
            trade_cache: VecDeque::new(),
            latency: LatencyModel::Fixed(20),
            fill_model: FillModel::PowerProbQueueFunc3(3.0),
            state: BacktestState::default(),
            depth: Depth::default(),
            next_tick: "".to_string(),
        }
    }

    pub async fn launch(
        &mut self,
        backtest_id: String,
        tick_url: &str,
    ) -> Result<(), std::io::Error> {
        self.prepare_data().await?;
        self.account.backtest_id = backtest_id;
        self.account.balance = self.config.balance.clone();
        // self.account.position.symbol = self.config.symbol.clone().split("_").next().unwrap().to_string();
        self.start_listening(tick_url).await;
        Ok(())
    }

    async fn prepare_data(&mut self) -> Result<(), std::io::Error> {
        let data_loader = self.data_loader.clone();
        let data_loader2 = self.data_loader.clone();
        let handle = tokio::spawn(async move { data_loader.lock().await.load_data().await });
        let handle2 = tokio::spawn(async move { data_loader2.lock().await.load_trade().await });

        // read the first block of data or the first file data into data_cache
        self.data_cache = VecDeque::from(handle.await??);
        if let Some(depth) = self.data_cache.pop_front() {
            self.depth = depth;
        }
        self.trade_cache = VecDeque::from(handle2.await??);
        self.state = BacktestState::Running;
        let tick = self.on_tick().await;
        self.next_tick = sonic_rs::to_string(&tick).expect("Failed to serialize tick data");
        Ok(())
    }

    pub async fn on_tick(&mut self) -> BacktestResponse {
        if self.state != BacktestState::Running {
            return BacktestResponse::bad_request("Backtest is not running".to_string());
        }

        if self.data_cache.is_empty() {
            let data_loader = self.data_loader.clone();
            // fill the cache with new data
            let handle = tokio::spawn(async move { data_loader.lock().await.load_data().await });
            match handle.await {
                Ok(Ok(data)) => {
                    self.data_cache = VecDeque::from(data);
                }
                Ok(Err(e)) => {
                    return BacktestResponse::bad_request(format!("Error loading data: {}", e));
                }
                Err(e) => {
                    return BacktestResponse::bad_request(format!("Error loading data: {}", e));
                }
            }
            // if no more data, return an end status
            if self.data_cache.is_empty() {
                self.state = BacktestState::Finished;
                // dont change the return message
                return BacktestResponse::bad_request("No more data, backtestfinished".to_string());
            }
        }
        if self.trade_cache.is_empty() && self.zconfig.use_trade{
            let data_loader = self.data_loader.clone();
            // fill the cache with new data
            let handle = tokio::spawn(async move { data_loader.lock().await.load_trade().await });
            match handle.await {
                Ok(Ok(data)) => {
                    self.trade_cache = VecDeque::from(data);
                }
                Ok(Err(e)) => {
                    return BacktestResponse::bad_request(format!("Error loading data: {}", e));
                }
                Err(e) => {
                    return BacktestResponse::bad_request(format!("Error loading data: {}", e));
                }
            }

            if self.trade_cache.is_empty() {
                self.state = BacktestState::Finished;
                // dont change the return message
                return BacktestResponse::bad_request("No more data, backtestfinished".to_string());
            }
        }
        let is_trade = matches!(
            self.trade_cache
                .front()
                .unwrap_or(&Trade{
                    local_timestamp: i64::MAX,
                    ..Default::default()
                })
                .local_timestamp
                .cmp(&self.data_cache.front().unwrap().local_timestamp),
            std::cmp::Ordering::Less
        );
        if is_trade {
            let trade = self.trade_cache.pop_front().unwrap_or_default();
            let tick_response = TickResponseTrade {
                trade,
                account: self.account.clone(),
                orders: self.order_list.clone(),
            };
            self.match_orders();
            return BacktestResponse::normal_response(sonic_rs::to_string(&tick_response).unwrap());
        }
        let mut depth = self.data_cache.pop_front().unwrap();
        for ask in depth.asks.iter_mut() {
            ask.1 = (ask.1 * 1e6).round() / 1e6;
        }
        for bids in depth.bids.iter_mut() {
            bids.1 = (bids.1 * 1e6).round() / 1e6;
        }
        // level 0 changed, trades happened, match orders
        self.depth = depth.clone();
        for position in self.account.position.iter_mut() {
            position.1.round();
            // check Forced Liquidation and stop loss
        }
        self.close_order_check(&self.depth.clone());
        let tick_response = TickResponseDepth {
            depth: self.depth.clone(),
            account: self.account.clone(),
            orders: self.order_list.clone(),
        };
        BacktestResponse::normal_response(sonic_rs::to_string(&tick_response).unwrap())
    }

    // check for forced liquidation and stop loss conditions
    fn close_order_check(&mut self, depth: &Depth) {
        let mut close_list: Vec<Order> = Vec::new();

        // Check Long positions
        let position_long = self.account.position.get_mut(&(
            depth.symbol.clone(),
            PositionSide::Long,
            depth.exchange,
        ));
        let mut total_amount = 0.0;
        let mid_price = (depth.asks[0].0 + depth.bids[0].0) / 2.0;

        if let Some(position) = position_long {
            // Sort the position by stop_loss price (ascending)
            position
                .stop_loss
                .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            for (index, loss) in position.stop_loss.iter_mut().enumerate() {
                total_amount += loss.1;
                if mid_price < loss.0 {
                    let stop_loss_order = Order {
                        contract_type: ContractType::Futures,
                        symbol: depth.symbol.clone(),
                        exchange: depth.exchange,
                        order_type: OrderType::Market,
                        position_side: PositionSide::Long,
                        leverage: position.leverage,
                        cid: "st-".to_string() + depth.symbol.clone().as_str(),
                        price: 0.0,
                        amount: loss.1,
                        side: OrderSide::Sell,
                        timestamp: 0,
                        ..Default::default()
                    };
                    close_list.push(stop_loss_order);
                }
                if total_amount >= position.amount_total {
                    // 只清除未遍历的部分
                    position.stop_loss.drain(index + 1..);
                    break;
                }
            }
        }

        // Check Short positions
        let position_short = self.account.position.get_mut(&(
            depth.symbol.clone(),
            PositionSide::Short,
            depth.exchange,
        ));
        total_amount = 0.0;

        if let Some(position) = position_short {
            // Sort the position by stop_loss price (descending)
            position
                .stop_loss
                .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            for (index, loss) in position.stop_loss.iter_mut().enumerate() {
                total_amount += loss.1;
                if mid_price > loss.0 {
                    let stop_loss_order = Order {
                        contract_type: ContractType::Futures,
                        symbol: depth.symbol.clone(),
                        exchange: depth.exchange,
                        order_type: OrderType::Market,
                        position_side: PositionSide::Short,
                        leverage: position.leverage,
                        cid: "st-".to_string() + depth.symbol.clone().as_str(),
                        price: 1e6,
                        amount: loss.1,
                        side: OrderSide::Sell,
                        timestamp: 0,
                        ..Default::default()
                    };
                    close_list.push(stop_loss_order);
                }
                if total_amount >= position.amount_total {
                    // Clear all stop-loss orders
                    position.stop_loss.drain(index + 1..);
                    break;
                }
            }
        }

        // Force close all remaining positions with market price if necessary
        for position_side in &[PositionSide::Long, PositionSide::Short] {
            if let Some(position) = self.account.position.get_mut(&(
                depth.symbol.clone(),
                position_side.clone(),
                depth.exchange,
            )) {
                let margin_need = (position.entry_price - depth.asks[0].0) * position.amount_total;
                if (position_side == &PositionSide::Short && margin_need < 0.0
                    || position_side == &PositionSide::Long && margin_need > 0.0)
                    && position.margin_value - margin_need.abs() < 0.0
                {
                    let fc_price = (depth.asks[0].0 + depth.bids[0].0) / 2.0;
                    let force_close_order = Order {
                        contract_type: ContractType::Futures,
                        symbol: depth.symbol.clone(),
                        exchange: depth.exchange,
                        order_type: OrderType::Market,
                        position_side: position_side.clone(),
                        leverage: position.leverage,
                        cid: "fc-".to_string() + depth.symbol.clone().as_str(),
                        price: fc_price,
                        amount: position.amount_total,
                        side: OrderSide::Sell,
                        timestamp: 0,
                        ..Default::default()
                    };
                    close_list.push(force_close_order);
                    position.amount_total = 0.0; // Ensure the total is reset
                    position.stop_loss.clear(); // Clear stop-loss
                }
            }
        }

        // Process all close orders
        for order in close_list {
            self.post_order(order);
        }
    }

    // return cid when success
    pub fn post_order(&mut self, mut order: Order) -> BacktestResponse {
        // check account balance, fix the amount and price
        if (order.amount - ((order.amount * 1e6).round() / 1e6)).abs() >= 1e-7 {
            return BacktestResponse::bad_request(
                "Invalid amount, position fix too small.".to_string(),
            );
        }
        if (order.price - ((order.price * 1e12).round() / 1e12)).abs() >= 1e-13 {
            return BacktestResponse::bad_request(
                "Invalid amount, position fix too small.".to_string(),
            );
        }
        if order.price <= 0.0 || order.amount <= 0.0 {
            return BacktestResponse::bad_request("Invalid order.".to_string());
        }
        // TODO: change front amount
        order.amount = (order.amount * 1e6).round() / 1e6;
        order.price = (order.price * 1e12).round() / 1e12;
        let amount = order.amount;
        let post_value = order.price * amount;
        // check margin for Spot
        if order.contract_type == ContractType::Spot {
            // Get the position for the symbol or insert a new one if it doesn't exist
            let position = self
                .account
                .position
                .entry((
                    order.symbol.clone(),
                    order.position_side.clone(),
                    order.exchange,
                ))
                .or_default();

            match order.side {
                OrderSide::Sell => {
                    if position.amount_available < order.amount {
                        return BacktestResponse::bad_request("Insufficient amount.".to_string());
                    }
                    position.add_freezed(order.amount);
                }
                OrderSide::Buy => {
                    self.account.balance.add_freezed(post_value);
                }
            }
        } else if order.contract_type == ContractType::Futures {
            // check margin for Perpetual
            let position = self
                .account
                .position
                .entry((
                    order.symbol.clone(),
                    order.position_side.clone(),
                    order.exchange,
                ))
                .or_insert_with(|| Position {
                    side: order.position_side.clone(),
                    exchange: order.exchange,
                    ..Default::default()
                });

            // increase leverage, decrease margin
            match order.side {
                OrderSide::Sell => {
                    if position.amount_available < order.amount {
                        // The corresponding quantity of orders with higher prices,
                        // such as the original order price of 100, now the order price of 50, the order of 100 needs to be canceled
                        let mut temp_vec: Vec<_> = self.order_list.inner.iter().cloned().collect();
                        temp_vec.sort_by(|a, b| {
                            b.price
                                .partial_cmp(&a.price)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        let mut amount_canceled = 0.0;
                        for order_inn in temp_vec.iter_mut() {
                            if order_inn.symbol == order.symbol
                                && order_inn.position_side == order.position_side
                                && order_inn.state == OrderState::Open
                                && order_inn.side == OrderSide::Sell
                            {
                                if order_inn.price < order.price {
                                    break;
                                }
                                if amount_canceled + (order_inn.amount - order_inn.filled_amount)
                                    >= order.amount
                                {
                                    self.cancel_order(order_inn.cid.clone());
                                    order_inn.amount = (order_inn.amount - order_inn.filled_amount)
                                        + amount_canceled
                                        - order.amount;
                                    order_inn.filled_amount = 0.0;
                                    self.post_order(order_inn.clone());
                                    self.post_order(order.clone());
                                    break;
                                }
                                amount_canceled += order_inn.amount - order_inn.filled_amount;
                                self.cancel_order(order_inn.cid.clone());
                            }
                        }
                        return BacktestResponse::bad_request("Insufficient amount, Canceled the amount out of position automatically.".to_string());
                    }
                    position.add_freezed(order.amount);
                    order.open_price = Some(position.entry_price);
                }
                OrderSide::Buy => {
                    let margin_value_need =
                        round6(order.amount / order.leverage as f64 * order.price);
                    if margin_value_need > self.account.balance.get_available() {
                        return BacktestResponse::bad_request("Insufficient margin.".to_string());
                    }
                    if let Some(loss) = order.stop_loss {
                        if (loss > order.price && order.position_side == PositionSide::Long)
                            || (loss < order.price && order.position_side == PositionSide::Short)
                        {
                            return BacktestResponse::bad_request(
                                "Stop loss invailed.".to_string(),
                            );
                        }
                        let mut found = false;
                        for loss_list in position.stop_loss.iter_mut() {
                            if loss_list.0 == loss {
                                loss_list.1 += order.amount;
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            position.stop_loss.push((loss, order.amount));
                        }
                    }
                    if let Some(profit) = order.take_profit {
                        if (profit < order.price && order.position_side == PositionSide::Long)
                            || (profit > order.price && order.position_side == PositionSide::Short)
                        {
                            return BacktestResponse::bad_request(
                                "Take profit invailed.".to_string(),
                            );
                        }
                    }
                    position.leverage = order.leverage;
                    if margin_value_need > 0.0 {
                        self.account.balance.add_freezed(margin_value_need);
                        order.margin = margin_value_need;
                    }
                }
            }
        }
        // update account
        let cid = order.cid.clone();
        // print!("{:?}, {:?}", post_value, amount);
        self.order_list
            .insert_order(self.latency.order_with_latency(order));

        BacktestResponse::normal_response(format!("cid: {} order posted.", cid))
    }

    pub fn cancel_order(&mut self, cid: String) -> BacktestResponse {
        // remove order from order_list
        let order = self.order_list.remove_order(cid);
        if let Some(mut order) = order {
            if order.state == OrderState::Filled || order.state == OrderState::Canceled {
                self.order_list.insert_order(order);
                return BacktestResponse::normal_response(
                    "Order already filled or Canceled.".to_string(),
                );
            }
            let mut amount = order.amount - order.filled_amount;
            let value = order.price * amount;
            amount = (amount * 1e6).round() / 1e6;
            if order.side == OrderSide::Buy {
                self.account
                    .balance
                    .sub_freezed(value / order.leverage as f64);
            } else {
                self.account
                    .position
                    .entry((
                        order.symbol.clone(),
                        order.position_side.clone(),
                        order.exchange,
                    ))
                    .or_default()
                    .sub_freezed(amount);
            }
            order.state = OrderState::Canceled;
            order.timestamp = self.depth.local_timestamp;
            let cid = order.cid.clone();
            // return an ok status response
            self.order_list.insert_order(order);
            BacktestResponse::normal_response(format!("cid: {} order canceled.", cid))
        } else {
            // cancel order not found
            // return an error status response
            BacktestResponse::bad_request("Order not found".to_string())
        }
    }

    fn match_orders(&mut self) {
        let depth = &mut self.depth.clone();
        // when tick update, try to match orders
        let filled_stack = self
            .order_list
            .execute_orders(depth, self.fill_model.clone());
        if !filled_stack.is_empty() {
            debug!("{:?}", filled_stack);
        }
        for filled in filled_stack {
            // info!("{:?}", filled);
            let account = self
                .account
                .position
                .entry((filled.symbol.clone(), filled.side.clone(), filled.exchange))
                .or_default();
            self.account.balance.fill_freezed(&filled);
            account.update_pos(&filled);
            info!("current orders: {:?}", self.order_list.inner);
            // take profit
            if filled.take_profit.is_some()
                && ((filled.filled_amount > 0.0 && filled.side == PositionSide::Long)
                    || (filled.filled_amount < 0.0 && filled.side == PositionSide::Short))
            {
                let take_profit_order = Order {
                    cid: format!("tp-{}", filled.cid),
                    exchange: filled.exchange,
                    symbol: filled.symbol.clone(),
                    position_side: filled.side.clone(),
                    contract_type: filled.contract_type.clone(),
                    side: OrderSide::Sell,
                    price: filled.take_profit.unwrap(),
                    amount: filled.filled_amount.abs(),
                    leverage: filled.leverage,
                    timestamp: depth.local_timestamp,
                    margin: filled.freeze_margin,
                    state: OrderState::Open,
                    ..Default::default()
                };
                self.post_order(take_profit_order);
            }
        }
        // self.account.judege_close((depth.bids[0].0 + depth.asks[0].0) / 2.0, depth.symbol.clone());
    }

    
    pub async fn close_bt(&mut self, responder: zmq::Socket, tick_url: &str) {
        info!(
            "Closing backtest server for id: {}.",
            self.account.backtest_id
        );
        responder
            .disconnect(format!("{}{}.ipc", tick_url, self.account.backtest_id).as_str())
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use log::info;

    use super::BtConfig;

    #[test]
    fn test_de_btconfig_from_str() {
        let config_str = r#"{
            "exchanges": ["BinanceSpot"],
            "symbol": "BTC_USDT",
            "start_time": 0,
            "end_time": 0,
            "balance": {
                "total": 0,
                "available": 0,
                "freezed": 0
            },
            "source": {"FilePath": "./data/BTCUSDT.csv"},
            "fee_rate": {
                "maker_fee": 0,
                "taker_fee": 0
            }
        }"#;
        let config = BtConfig::parse(config_str);
        info!("{:?}", config);
    }
    use super::ZileanV1;

    #[tokio::test]
    async fn test_on_tick() {
        let zconfig = crate::ZConfig::parse("misc/config.toml");
        let mut zilean = ZileanV1::new(BtConfig::default(), zconfig).await;
        println!("{:?}", BtConfig::default());
        zilean.state = super::BacktestState::Running;
        zilean.prepare_data().await.unwrap();
        zilean.on_tick().await;
    }
}
