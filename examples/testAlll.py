import zmq
import json
import time

# 创建 ZMQ 上下文
context = zmq.Context()

def send_and_receive(socket, message):
    """
    向服务器发送请求并接收响应。
    
    参数:
        socket (zmq.Socket): 与服务器通信的 ZMQ 套接字。
        message (dict): 要发送的 JSON 格式的消息。
    
    返回:
        dict: 服务器的响应，JSON 格式。
    """
    socket.send(json.dumps(message).encode('utf-8'))
    reply = socket.recv()
    print(reply)
    return json.loads(reply.decode('utf-8'))

def init_backtest(socket, backtest_message):
    """
    初始化回测，并从服务器获取回测 ID。
    
    参数:
        socket (zmq.Socket): 与服务器通信的 ZMQ 套接字。
        backtest_message (dict): 包含回测参数的 JSON 格式消息。
    
    返回:
        str: 回测 ID。
    """
    launch_message = 'LAUNCH_BACKTEST'.encode('utf-8') + json.dumps(backtest_message).encode('utf-8')
    socket.send(launch_message)
    reply = socket.recv()
    data = json.loads(reply.decode('utf-8'))
    
    if data.get('status') == 'ok':
        return data.get('backtest_id')
    else:
        raise Exception("Failed to launch backtest: {}".format(data.get('message')))

def get_data(socket, command):
    """
    向服务器请求数据，如 orderbook 或账户信息。
    
    参数:
        socket (zmq.Socket): 与服务器通信的 ZMQ 套接字。
        command (str): 要请求的命令。
    
    返回:
        dict: 服务器的响应，JSON 格式。
    """
    socket.send(command.encode('utf-8'))
    reply = socket.recv()
    return json.loads(reply.decode('utf-8'))

def post_order(socket, exchange, cnt, symbol, price, amount, side="Buy"):
    """
    发送下单请求。

    参数:
        exchange (str): 交易所名称，如 "BinanceSpot"。
        socket (zmq.Socket): 与服务器通信的 ZMQ 套接字。
        cnt (int): 当前的计数器，用于唯一订单 ID, 请保证ID唯一, 否则撤单会出现错误。
        symbol (str): 交易对，如 "BTC_USDT"。
        price (float): 订单的价格。
        amount (float): 订单的数量。
        side (str): 交易方向，"Buy" 或 "Sell"。
    
    返回:
        dict: 服务器响应，包含订单状态。
    """
    order_message = {
        "exchange": exchange,
        "cid": str(cnt),  # 唯一订单 ID
        "symbol": symbol,
        "price": price,
        "amount": amount,
        "filed_amount": 0.0,  # 假设初始成交量为 0
        "avg_price": price,  # 初始平均价格等于下单价格
        "side": side,  # 买单或卖单
        "state": "Open",  # Open 开单
        "order_type": "Limit",  # 假设为限价单
        "time_in_force": "Gtc",  # 默认 'Good-Till-Cancelled'
        "timestamp": int(time.time() * 1000),  # 当前时间戳（毫秒）
    }
    socket.send('POST_ORDER'.encode('utf-8') + json.dumps(order_message).encode('utf-8'))
    reply = socket.recv()
    return json.loads(reply.decode('utf-8'))

def cancel_order(socket, cid):
    """
    发送下单请求。

    参数:
        cnt (int): 用于撤单的ID，用于唯一订单 ID。
    """
    socket.send('CANCEL_ORDER'.encode('utf-8') + cid.encode('utf-8'))
    reply = socket.recv()
    return json.loads(reply.decode('utf-8'))

# 连接回测服务器
print("Connecting to Zilean backtest server...")
socket_server = context.socket(zmq.REQ)
socket_server.connect("ipc:///tmp/zilean_backtest.ipc")

# 定义回测参数
backtest_params = {
    "exchanges": ["BinanceSpot"],
    "symbol": "BTC_USDT",
    "start_time": 0,
    "end_time": 1728885047114,
    "balance": {
        "total": 100000000,
        "available": 100000000,
        "freezed": 0
    },
    "source": "Database",
    "fee_rate": {
        "maker_fee": 0,
        "taker_fee": 0
    }
}

# 启动回测并获取回测 ID
backtest_id = init_backtest(socket_server, backtest_params)
print(f"Backtest launched with ID: {backtest_id}")

# 连接到具体回测的通信接口
socket_bt = context.socket(zmq.REQ)
socket_bt.connect(f"ipc:///tmp/zilean_backtest/{backtest_id}.ipc")

# 执行多次数据请求与处理
cnt = 0
start_time = time.time()

while cnt < 1000000:
    # 获取下一个时间点的 orderbook 数据
    try:
        orderbook = get_data(socket_bt, 'TICK')
    except:
        for i in range(cnt):
            cancel_order(socket_bt, cnt)
    
    message = json.loads(orderbook.get('message', {}).encode('utf-8'))
    asks = message.get('asks', [])
    bids = message.get('bids', [])

    if asks and bids and cnt % 1000 == 0:
        # 获取最高买价和最低卖价进行下单测试
        best_bid = float(bids[0][0])  # 最高买价
        best_ask = float(asks[0][0])  # 最低卖价

        print(f"Best bid: {best_bid}, Best ask: {best_ask}")

        # 测试下单逻辑：以最低卖价买入
        order_response = post_order(socket_bt, "BinanceSpot", cnt, "BTC_USDT", best_ask-0.1, 1.0, side="Buy")
        # 测试下单逻辑：以最高卖价卖出
        order_response = post_order(socket_bt, "BinanceSpot", cnt, "BTC_USDT", best_bid+0.1, 1.0, side="Sell")
        print(f"Order Response: {order_response}")
    
        # 获取账户信息
        acc_info = get_data(socket_bt, 'GET_ACCOUNT_INFO')
        print(acc_info.get('message'))

        # cancel_order(bt_socket, cnt)
        # 检查账户信息是否正确
        if acc_info.get('status') != 'ok':
            print('Error fetching account info')
            break
    cnt += 1

        

print(f"Processed {cnt} ticks in {time.time() - start_time} seconds")
