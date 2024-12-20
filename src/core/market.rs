use crate::engine::*;
use clickhouse::Row;
use serde::ser::Serializer;
use sonic_rs::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Exchange {
    #[default]
    BinanceSpot,
    CoinbaseSpot,
    OkxSpot,
    KrakenSpot,
    BinanceSwap,
    BybitSwap,
    BinanceSwapDec,
    BinanceSwapInc,
    BitgetSwap,
}

// version1.0 only support limit order
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum OrderType {
    #[default]
    Limit,
    Market,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum TimeInForce {
    #[default]
    Gtc,
    Ioc,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
pub enum OrderSide {
    #[default]
    Buy,
    Sell,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub enum OrderState {
    #[default]
    Open,
    Canceled,
    PartiallyFilled,
    Filled,
}
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub enum ContractType {
    #[default]
    Spot, // 现货
    Futures,    // 期货(永续合约)
    // Options,    // 期权
    // Perpetual, // 永续合约
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum MarginMode {
    Cross, // 全仓
    #[default]
    Isolated, // 逐仓
}

// 用于平仓
pub struct ClosePosition {
    pub symbol: String,
    pub amount: f64,
    pub price: f64,
    pub side: OrderSide,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Balance {
    total: f64,
    available: f64,
    freezed: f64,
}

impl Balance {
    pub fn get_available(&self) -> f64 {
        self.available
    }

    pub fn add_freezed(&mut self, value: f64) {
        self.freezed += value;
        self.available -= value;
    }

    pub fn sub_freezed(&mut self, value: f64) {
        self.freezed -= value;
        self.available += value;
    }
    pub fn fill_freezed(&mut self, filled: &FilledStack){
        match filled.contract_type {
            ContractType::Spot => self.fill_freezed_spot(filled),
            ContractType::Futures => self.fill_freezed_futures(filled),
        }
    }
    pub fn fill_freezed_futures(&mut self, filled: &FilledStack) {
        let amount = filled.filled_amount;
        let value = (filled.filled_price * filled.filled_amount / filled.leverage as f64 * 1000000.0).round()/1000000.0;
        let freeze = (filled.post_price * filled.filled_amount / filled.leverage as f64 * 1000000.0).round()/1000000.0;
        // println!("fill_price: {}, fill_amount: {}, fill_value: {}, freeze_price: {}, freeze_value: {}, lev:{}", filled.filled_price, filled.filled_amount, value, filled.post_price, freeze, filled.leverage);
        // println!("fill_freezed: price: {}, amount: {}, freeze_price: {}", value, amount, freeze);
        if filled.side == PositionSide::Long {
            // open long:amount>0  / close long: amount<0 
            self.total -= value;
            if amount > 0.0 {
                // println!("filled.freeze_mar: {}, amount: {}, amount_tol: {}", filled.freeze_margin, amount, filled.amount_total);
                // println!("fill:{:?}", filled);
                // println!("subFreeze,{}", filled.freeze_margin * (amount / filled.amount_total));
                self.freezed -= freeze;
                self.available += freeze - value;
            } else {
                self.available -= value;
            }
        } else if filled.side == PositionSide::Short {
            // open short:amount<0  / close short: amount>0 
            self.total += value;
            // println!("value: {}, freeze: {}, available: {}", value, freeze, self.available);
            if amount < 0.0 {
                self.freezed += freeze;
                self.available -= freeze - value;
            } else {
                self.available += value;
            }
        }
        self.round();
        // self.freezed -= filled.freeze_margin * (amount / filled.amount_total);
        // // buy futures, amount > 0, available increase
        // // open long \ close shot \ open shot \ close long
        // self.available += filled.freeze_margin * (amount / filled.amount_total) - price * amount;
    }
    pub fn fill_freezed_spot(&mut self, filled: &FilledStack) {
        // buy coin, amount > 0
        // debug!("fill_freezed: price: {}, amount: {}, freeze_price: {}", price, amount, freeze_price);
        let price = filled.filled_price;
        let amount = filled.filled_amount;
        let freeze_price = filled.post_price;
        self.total -= price * amount;
        // buy coin, amount > 0, available increase
        if amount > 0.0 {
            self.freezed -= freeze_price * amount;
            self.available += (freeze_price - price) * amount;
        } else {
            self.available -= price * amount;
        }
    }

    pub fn round(&mut self) {
        self.freezed = (self.freezed * 10000000.0).round() / 10000000.0;
        self.available = (self.available * 10000000.0).round() / 10000000.0;
        self.total = (self.total * 10000000.0).round() / 10000000.0;
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct FeeRate {
    maker_fee: f64,
    taker_fee: f64,
}

// version1.0 only support spot position, so no position side
#[derive(Debug, Default, Serialize, Clone, Hash, Eq, PartialEq, Deserialize)]
pub enum PositionSide {
    #[default]
    Long,
    Short,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct Account {
    pub backtest_id: String,
    pub balance: Balance,
    #[serde(serialize_with = "serialize_position_map")]
    pub position: HashMap<(String, PositionSide, Exchange), Position>, // String: Symbol_ContractType
}
fn serialize_position_map<S>(
    position_map: &HashMap<(String, PositionSide, Exchange), Position>, 
    serializer: S
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map: HashMap<String, Vec<Position>> = HashMap::new();

    // Iterate over position_map and group positions by symbol
    for ((symbol, _side, _exchange), position) in position_map {
        // Insert the position into the Vec associated with the symbol key
        map.entry(symbol.clone())
            .or_default()
            .push(position.clone());  // Add the position to the vector
    }

    // Serialize the resulting map
    map.serialize(serializer)
}


#[derive(Debug, Default, Serialize, Clone)]
// only support one exchange and one symbol
pub struct Position {
    pub side: PositionSide,
    pub exchange: Exchange,
    pub leverage: u32,
    pub margin_value: f64,
    pub amount_total: f64,
    pub amount_available: f64,
    pub amount_freezed: f64,
    pub entry_price: f64,
    // entry_time: i64,
    pub stop_loss: Vec<(f64, f64)>,
}

impl Position {
    pub fn update_pos(&mut self, filled : &FilledStack) {
        self.round();
        let price = filled.filled_price;
        let amount = filled.filled_amount;
        let old_value = self.amount_total * self.entry_price;

        if filled.contract_type == ContractType::Futures {
            if filled.side == PositionSide::Long {
                // open long:amount>0 / close long: amount<0
                self.amount_total += amount;
                if amount>0.0{ // open long
                    self.amount_available += amount;
                }else{ // open long
                    self.amount_freezed += amount;
                }
            }else if filled.side == PositionSide::Short {
                // println!("{:?}", amount);
                self.amount_total -= amount;
                if amount<0.0{ // open short
                    self.amount_available -= amount;
                }else{ // close short
                    self.amount_freezed -= amount;
                }
            }
            
            if self.amount_total == 0.0 {
                self.entry_price = 0.0;
            } else {
                self.entry_price = (old_value + amount.abs() * price) / self.amount_total;
            }
            self.margin_value = self.amount_total * self.entry_price / filled.leverage as f64;
        }else if filled.contract_type == ContractType::Spot {
            self.amount_total += amount;
            if amount > 0.0 {
                self.amount_available += amount;
            } else {
                self.amount_freezed += amount;
            }

            if self.amount_total == 0.0 {
                self.entry_price = 0.0;
            } else {
                self.entry_price = (old_value + amount * price) / self.amount_total;
            }
        }
    }

    pub fn add_freezed(&mut self, value: f64) {
        self.amount_freezed += value;
        self.amount_available -= value;
    }

    pub fn sub_freezed(&mut self, value: f64) {
        self.amount_freezed -= value;
        self.amount_available += value;
    }

    // buy order fill, value > 0
    // pub fn fill_freezed(&mut self, value: f64) {
        
    // }

    pub fn round(&mut self) {
        self.amount_freezed = (self.amount_freezed * 100000.0).round() / 100000.0;
        self.amount_available = (self.amount_available * 100000.0).round() / 100000.0;
        self.amount_total = (self.amount_total * 100000.0).round() / 100000.0;
        if self.entry_price.is_nan() || self.entry_price.is_infinite() {
            self.entry_price = 0.0;
        }
    }
}

// skip: ignore this field when serialize, single exchange don't need this field
// default on spot
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Order {
    // for futures
    #[serde(default)]
    pub contract_type: ContractType,
    #[serde(default)]
    pub position_side: PositionSide,
    // New fields for contract margin mode and leverage
    #[serde(default)]
    pub margin_mode: MarginMode, // Margin mode: Cross or Isolated
    #[serde(default = "default_leverage")]
    pub leverage: u32, // set leverage multiplier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>, // Take profit price
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>, // Stop loss price
    #[serde(default)]
    pub reduce_only: bool, // default false

    #[serde(skip_serializing)]
    pub exchange: Exchange,
    pub cid: String,
    #[serde(skip_serializing)]
    pub symbol: String,
    pub price: f64,
    pub amount: f64,
    pub filled_amount: f64,
    #[serde(default = "default_amount", skip)]
    pub front_amount: f64,
    #[serde(default = "default_depth", skip)]
    pub prev_depth: Depth,
    pub avg_price: f64,
    pub side: OrderSide,
    pub state: OrderState,
    #[serde(skip_serializing)]
    pub order_type: OrderType,
    #[serde(skip)]
    pub margin: f64,
    #[serde(skip_serializing)]
    pub time_in_force: TimeInForce,
    pub timestamp: i64,
}
fn default_leverage() -> u32 {
    1
}
fn default_amount() -> f64 {
    -1.0
}
fn default_depth() -> Depth {
    Depth::default()
}

impl Order {
    pub fn execute(&mut self, depth: &Depth, fill_model: FillModel) -> (f64, f64) {
        let mut avg_price = 0.0;
        let mut executed_amount = 0.0;
        let mut executed_value = 0.0;
        let mut rest_amount = self.amount - self.filled_amount;
        let mut pos_coefficient = 1.0;
        if (self.side == OrderSide::Buy && self.position_side == PositionSide::Long && self.contract_type == ContractType::Futures)
            || (self.side == OrderSide::Sell && self.position_side == PositionSide::Short && self.contract_type == ContractType::Futures)
                || (self.side == OrderSide::Buy && self.contract_type == ContractType::Spot){
                // Maintain an execution position queue
                if depth.asks[0].0 > self.price && rest_amount > 0.0 {
                    let exe_amount = self.update_front_amount(depth, fill_model);
                    if exe_amount != 0.0 {
                        executed_amount += exe_amount;
                        executed_value += exe_amount * self.price;
                        avg_price = executed_value / executed_amount;
                    }
                } else {
                    for ask in depth.asks.iter() {
                        if ask.0 > self.price || rest_amount <= 0.0 {
                            break;
                        }
                        self.front_amount = 0.0;
                        let amount_to_execute = rest_amount.min(ask.1);
                        executed_amount += amount_to_execute;
                        executed_value += amount_to_execute * ask.0;
                        avg_price = executed_value / executed_amount;
                        rest_amount -= amount_to_execute;
                    }
                }
        }else if (self.side == OrderSide::Buy && self.position_side == PositionSide::Short && self.contract_type == ContractType::Futures)
        || (self.side == OrderSide::Sell && self.position_side == PositionSide::Long && self.contract_type == ContractType::Futures)
        || (self.side == OrderSide::Sell && self.contract_type == ContractType::Spot)
        {
            if depth.bids[0].0 < self.price && rest_amount > 0.0 {
                let exe_amount = self.update_front_amount(depth, fill_model);
                if exe_amount != 0.0 {
                    executed_amount += exe_amount;
                    executed_value += exe_amount * self.price;
                    avg_price = executed_value / executed_amount;
                }
            } else {
                for bid in depth.bids.iter() {
                    if bid.0 < self.price || rest_amount <= 0.0 {
                        break;
                    }
                    let amount_to_execute = rest_amount.min(bid.1);
                    executed_amount += amount_to_execute;
                    executed_value += amount_to_execute * bid.0;
                    avg_price = executed_value / executed_amount;
                    rest_amount -= amount_to_execute;
                }
            }
            pos_coefficient = -1.0;
        }
                

        // this is out of most max precision of post, so it won't change the result
        executed_amount = (executed_amount * 1000000.0).round() / 1000000.0;
        avg_price = (avg_price * 100000.0).round() / 100000.0;
        if avg_price > 0.0 {
            self.avg_price = (avg_price * executed_amount + self.avg_price * self.filled_amount)
                / (self.filled_amount + executed_amount);
        }
        self.filled_amount += executed_amount;

        if self.filled_amount == self.amount {
            self.state = OrderState::Filled;
        } else if self.filled_amount > 0.0 {
            self.state = OrderState::PartiallyFilled;
        }
        (avg_price, executed_amount * pos_coefficient)
    }

    // ------------- return amount to be filled -------------------
    #[allow(clippy::needless_return)]
    fn update_front_amount(&mut self, new_depth: &Depth, fill_model: FillModel) -> f64 {
        // init amount
        if self.front_amount < 0.0 {
            self.front_amount = 0.0;
            match self.side {
                OrderSide::Buy => {
                    for ask in new_depth.asks.iter() {
                        if ask.0 <= self.price {
                            self.front_amount = ask.1;
                            continue;
                        }
                        break;
                    }
                }
                OrderSide::Sell => {
                    for bid in new_depth.bids.iter() {
                        if bid.0 >= self.price {
                            self.front_amount = bid.1;
                            continue;
                        }
                        break;
                    }
                }
            }
            self.prev_depth = new_depth.clone();
            return 0.0;
        }
        let mut prev_amount = 0.0;
        let mut new_amount = 0.0;
        match self.side {
            OrderSide::Buy => {
                for ask in new_depth.asks.iter() {
                    if ask.0 <= self.price {
                        new_amount = ask.1;
                        continue;
                    }
                    break;
                }
                for ask in self.prev_depth.asks.iter() {
                    if ask.0 <= self.price {
                        prev_amount = ask.1;
                        continue;
                    }
                    break;
                }
            }
            OrderSide::Sell => {
                for bid in new_depth.bids.iter() {
                    if bid.0 >= self.price {
                        new_amount = bid.1;
                        continue;
                    }
                    break;
                }
                for bid in self.prev_depth.bids.iter() {
                    if bid.0 >= self.price {
                        prev_amount = bid.1;
                        continue;
                    }
                    break;
                }
            }
        };
        let chg = prev_amount - new_amount;
        if chg < 0.0 {
            self.front_amount = self.front_amount.min(new_amount);
            return 0.0;
        }

        let front = self.front_amount;
        let back = prev_amount - front;

        let mut prob = fill_model.prob(back, front);
        if prob > 1.0 || prob.is_infinite() {
            prob = 1.0;
        }
        let new_front = front - (1.0 - prob) * chg + (back - prob * chg).min(0.0);
        self.front_amount = new_front.min(new_amount).min(0.0);
        // match success on front amount update
        if self.front_amount == 0.0
            && ((self.side == OrderSide::Buy && (self.price - new_depth.bids[0].0).abs() < 1e-6)
                || (self.side == OrderSide::Sell
                    && (new_depth.asks[0].0 - self.price).abs() < 1e-6))
        {
            let mut ret = chg;

            if chg > self.amount - self.filled_amount {
                ret = self.amount - self.filled_amount;
            }
            ret = (ret * 100000.0).round() / 100000.0;
            return ret;
        }
        return 0.0;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Level {
    price: f64,
    amount: f64,
}

#[derive(Default, Deserialize, Serialize, Row, Debug, Clone)]
pub struct Depth {
    #[serde(deserialize_with = "deserialize_exchange")]
    pub exchange: Exchange,
    pub symbol: String,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub timestamp: i64,
    pub local_timestamp: i64,
}

fn deserialize_exchange<'de, D>(deserializer: D) -> Result<Exchange, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.as_str() {
        "binance" => Ok(Exchange::BinanceSpot),
        "coinbase" => Ok(Exchange::CoinbaseSpot),
        "kraken" => Ok(Exchange::KrakenSpot),
        "okx" => Ok(Exchange::OkxSpot),
        "okex" => Ok(Exchange::OkxSpot),
        "binance-futures" => Ok(Exchange::BinanceSwap),
        "BinanceSpot" => Ok(Exchange::BinanceSpot),
        "CoinbaseSpot" => Ok(Exchange::CoinbaseSpot),
        "KrakenSpot" => Ok(Exchange::KrakenSpot),
        "OkxSpot" => Ok(Exchange::OkxSpot),
        "OkexSpot" => Ok(Exchange::OkxSpot),
        "bybit" => Ok(Exchange::BybitSwap),
        "bitget_futures" => Ok(Exchange::BitgetSwap),
        // 更多匹配
        _ => Err(serde::de::Error::custom(format!("Unknown exchange: {}", s))),
    }
}

#[derive(Default, Debug, Deserialize, Serialize, Clone, Row)]
pub struct Trade {
    pub exchange: String,           
    pub symbol: String,               
    pub timestamp: i64,      
    pub local_timestamp: i64,        
    pub id: u64,                   
    pub price: f64,           
    pub amount: f64,          
    pub side: String,                 
}