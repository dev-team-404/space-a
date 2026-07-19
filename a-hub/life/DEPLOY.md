# room_server 배포 — Oracle Cloud VM (현재 운영 중)

room_server를 **사외에서 접근 가능한 테스트 서버**로 띄운 기록·운영 절차입니다.
room_server는 인메모리 + SQLite 단일 프로세스라 **상주 서버**로 운영합니다.

## 현재 배포 상태

| 항목 | 값 |
|---|---|
| Base URL | `http://158.179.194.42:8001` |
| OpenAPI 문서 | `http://158.179.194.42:8001/docs` |
| VM | Oracle Cloud (OCI), Ubuntu 24.04 LTS, AMD x86_64, RAM ~1 GB |
| SSH | `ssh msalt-spacea` (User `ubuntu`, HostName `158.179.194.42`) |
| 배포 경로 | `~/space-a-life/life` |
| 런타임 | Docker + Compose (`space-a-room-server` 컨테이너, `restart: unless-stopped`) |
| 인증 | `x-api-key` 관문 활성 (`ROOM_SERVER_API_KEY`) |
| 영속 | SQLite 볼륨 `life_room-server-data` (`/data/rooms.db`) — 재부팅에도 유지 |
| 스왑 | 2 GB (`/swapfile`, 빌드 OOM 방지) |

> ⚠️ 평문 HTTP라 **토큰·`x-api-key`가 네트워크에 그대로 흐릅니다.** 테스트 용도로만 쓰고,
> 운영이 필요해지면 Caddy/Cloudflare Tunnel로 HTTPS를 앞에 두세요.

## 접속 확인

로컬 PC에서:

```bash
API=http://158.179.194.42:8001
KEY=<x-api-key>

curl $API/healthz                        # 관문 면제 → {"status":"ok"} (키 없이 200)
curl $API/ -H "x-api-key: $KEY"          # discovery — api_key_required: true
curl -i $API/capabilities                # 키 없이 → 401 unauthorized
curl -X POST $API/rooms/register -H "x-api-key: $KEY" \
  -H 'content-type: application/json' -d '{"name":"bot"}'
```

## 코드 갱신 (재배포)

리포가 private이라 클론 대신 로컬 `a-hub/life/` 폴더를 VM으로 전송한다.
저장소 루트에서:

```bash
cd a-hub
# life 폴더를 VM으로 전송 (.git·캐시 제외)
tar czf - --exclude='__pycache__' --exclude='.pytest_cache' --exclude='*.egg-info' --exclude='.git' life \
  | ssh msalt-spacea 'tar xzf - -C ~/space-a-life'
# 재빌드·재기동
ssh msalt-spacea 'cd ~/space-a-life/life && sudo ROOM_SERVER_API_KEY="<x-api-key>" docker compose up -d --build'
```

## 운영 메모

`ssh msalt-spacea` 접속 후 `cd ~/space-a-life/life` 에서:

| 작업 | 명령 |
|---|---|
| 상태 | `sudo docker compose ps` |
| 로그 | `sudo docker compose logs -f` |
| 재시작 | `sudo docker compose restart` |
| 키 변경 | `sudo ROOM_SERVER_API_KEY="<새키>" docker compose up -d` |
| 중지 | `sudo docker compose down` (볼륨 유지) |
| 데이터 초기화 | `sudo docker compose down -v && sudo ROOM_SERVER_API_KEY="<키>" docker compose up -d` |
| DB 백업 | `sudo docker cp space-a-room-server:/data/rooms.db ./rooms.db` |

`restart: unless-stopped`라 VM 재부팅 후 Docker 데몬이 뜨면 컨테이너도 자동 복귀한다.

## 처음 셋업 시 했던 것 (재현용)

새 VM에 다시 올려야 할 때 참고. 위 "현재 배포 상태"는 이 절차의 결과다.

### 1. 스왑 2 GB (RAM ~1 GB라 빌드 OOM 방지)

```bash
ssh msalt-spacea '
  sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile
  sudo mkswap /swapfile && sudo swapon /swapfile
  echo "/swapfile none swap sw 0 0" | sudo tee -a /etc/fstab'
```

### 2. Docker 설치

```bash
ssh msalt-spacea 'curl -fsSL https://get.docker.com | sudo sh && sudo usermod -aG docker $USER'
```

### 3. 코드 전송 & 기동

위 [코드 갱신](#코드-갱신-재배포) 절차와 동일. 최초엔 `mkdir -p ~/space-a-life` 후 전송.

### 4. 포트 8001 열기 — 두 군데 다

OCI는 **클라우드 방화벽(Security List/NSG)** 과 **VM 안 iptables** 를 둘 다 통과해야 한다.

**4-1. VM iptables** (Ubuntu OCI 이미지는 REJECT 규칙이 기본):

```bash
ssh msalt-spacea '
  sudo iptables -I INPUT -p tcp --dport 8001 -j ACCEPT   # 체인 맨 앞에 삽입 → REJECT보다 먼저 적용
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y iptables-persistent
  sudo netfilter-persistent save'
```

**4-2. OCI Security List** (콘솔 — SSH로 불가, 직접):

`Networking → VCN → Subnet → Security List → Ingress Rules → Add`

| 필드 | 값 |
|---|---|
| Source CIDR | `0.0.0.0/0` (또는 접근할 IP만) |
| IP Protocol | TCP |
| Destination Port | `8001` |

> NSG를 쓰는 인스턴스면 Security List 대신 해당 NSG에 같은 규칙을 추가한다.
