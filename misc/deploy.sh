#!/bin/bash
set -exu

# 默认参数
UPLOAD_SERVICE=false
START_SERVICE=false

# 解析参数
while getopts "su" opt; do
  case $opt in
    u)
      UPLOAD_SERVICE=true
      ;;
    s)
      START_SERVICE=true
      ;;
    *)
      echo "Invalid option: -$OPTARG" >&2
      exit 1
      ;;
  esac
done

# 变量定义
r=192.168.1.154
bin=zilean

# 编译
cargo b -r

# 上传可执行文件到 /root/bin
rsync -avz --progress target/release/$bin bw@$r:/tmp/bin/$bin
# ssh $r "sudo mv /tmp/$bin /root/bin/"

# 如果指定了 -s 参数，则上传服务文件
if [ "$UPLOAD_SERVICE" = true ]; then
  rsync -avz --progress zilean.service bw@$r:/tmp/zilean.service
  ssh $r "sudo mv /tmp/zilean.service /etc/systemd/system/"
  ssh $r "sudo systemctl daemon-reload"
fi

# 如果指定了 -u 参数，则启动服务
if [ "$START_SERVICE" = true ]; then
  sshpass -p "1332192071" ssh bw@$r "sudo systemctl restart $bin"
fi
