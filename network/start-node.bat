@echo off
:: ============================================================
::  TKS Node Launcher for Windows
::  Joins TKS Network via DNS seeds — no IP address needed
:: ============================================================

setlocal

set BINARY=%~dp0tks-chain-node.exe
set CHAINSPEC=%~dp0tks-testnet-spec-raw.json
set DATA_DIR=%USERPROFILE%\.tks\data
set NODE_NAME=TKS-Node-%COMPUTERNAME%

echo ============================================
echo   TKS Network Node (Windows)
echo   Name:  %NODE_NAME%
echo   Data:  %DATA_DIR%
echo   Seeds: 6 DNS seed nodes (no IP needed)
echo ============================================
echo.

if not exist "%DATA_DIR%" mkdir "%DATA_DIR%"

"%BINARY%" ^
  --chain "%CHAINSPEC%" ^
  --bootnodes "/dns/seed.tokenkickstarter.com/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C" ^
  --bootnodes "/dns/seed.tkstoken.com/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR" ^
  --bootnodes "/dns/seed.tksscan.com/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8" ^
  --bootnodes "/dns/seed.tokenkickstarter.ink/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C" ^
  --bootnodes "/dns/seed.tokenkickstarter.xyz/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR" ^
  --bootnodes "/dns/seed.tokenkickstarter.pw/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8" ^
  --base-path "%DATA_DIR%" ^
  --port 30333 ^
  --rpc-port 9944 ^
  --rpc-external ^
  --rpc-cors all ^
  --name "%NODE_NAME%" ^
  --log info

pause
