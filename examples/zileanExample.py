import zmq
import time
import json
import msgpack

context = zmq.Context()

def send_and_receive(socket, request):
    socket.send(request, copy=False)
    reply = socket.recv(copy=False)
    message = reply.bytes.decode('utf-8')
    return json.loads(message)  

def main():
    print("Connecting to backtest server…")

    socket_server = context.socket(zmq.REQ)
    socket_server.connect("ipc:///tmp/zilean_backtest.ipc")

    message = {
        "exchanges": ["BinanceSpot"],
        "symbol": "BTC_USDT",
        "start_time": 0,
        "end_time": 1727930047114,
        "balance": {"total": 0, "available": 0, "freezed": 0},
        "source": "Database",
        "fee_rate": {"maker_fee": 0, "taker_fee": 0},
    }

    request = 'LAUNCH_BACKTEST'.encode('utf-8') +json.dumps(message).encode('utf-8') 
    data = send_and_receive(socket_server, request)
    print(data)

    if data.get('status') == 'ok':
        backtest_id = data.get('backtest_id')
        print('backtest_id:', backtest_id)

        socket_bt = context.socket(zmq.REQ)
        socket_bt.connect(f"ipc:///tmp/zilean_backtest/{backtest_id}.ipc")

        cnt = 0
        start_time = time.time()  # 记录开始时间

        while True:
            request = f'TICK{backtest_id}'.encode('utf-8')
            response = send_and_receive(socket_bt, request)

            # 处理响应
            if cnt % 10000 == 0:
                print(f"{cnt} {response['status']} {response['message'][13:20]}")
            cnt += 1

            if response.get('status') != 'ok':
                print('Backtest error')  # 可能是数据结束
                break

        elapsed_time = time.time() - start_time
        print(f"程序运行时间: {elapsed_time} 秒")

# 启动异步事件循环
if __name__ == "__main__":
    main()