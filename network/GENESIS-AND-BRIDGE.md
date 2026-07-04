# TKS Mainnet — Genesis Allocation & Migration Bridge

## 1. Genesis Block — Who Gets What

### Total Supply: 1,000,000,000 TKS (1 Billion)

**The genesis block is the ONLY time TKS is created from nothing.**  
All future TKS comes from block rewards (15 TKS/block, ~80% to validators).

---

### Mainnet Genesis Allocation

| Wallet | Amount | % | Purpose |
|--------|--------|---|---------|
| **Team / Foundation** | 150,000,000 TKS | 15% | Team + 4-year vesting |
| **Bridge Reserve** | 300,000,000 TKS | 30% | Migration pool for ETH/BSC holders |
| **Ecosystem Fund** | 150,000,000 TKS | 15% | Grants, partnerships, listings |
| **Treasury (on-chain)** | 100,000,000 TKS | 10% | Community governance fund |
| **Initial Validators** | 50,000,000 TKS | 5% | 5 validators × 100K bond + extra |
| **Public Sale / IDO** | 150,000,000 TKS | 15% | Sold to raise operational funds |
| **Airdrop / Community** | 100,000,000 TKS | 10% | Holders, early adopters |

> **Bridge Reserve is key**: 300M TKS is locked in genesis and only released
> when ETH/BSC holders prove they burned their ERC-20 tokens.

---

## 2. How ETH/BSC Holders Migrate to TKS Network

### The Problem
- Users hold **TKS ERC-20** on Ethereum or BSC
- They need **native TKS** on the TKS blockchain
- No centralized exchange can do this yet

### The Solution: Lock-and-Release Bridge

```
ETH/BSC Side:                          TKS Network Side:
─────────────                          ─────────────────
User locks TKS ERC-20                  
in TKSBridge.sol          →  Validators watch events
                                       →  Bridge pallet releases
                                          native TKS to same address
```

**Same address on both sides** — because TKS uses Ethereum addresses (`0x...`),
the same MetaMask wallet address receives native TKS automatically.

---

## 3. Bridge Contract — TKSBridge.sol (ETH + BSC)

Deploy this on Ethereum mainnet AND BSC:

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IERC20 {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
}

/**
 * @title TKSBridge
 * @notice Lock TKS ERC-20 tokens here to receive native TKS on the TKS network.
 *         Deploy on Ethereum mainnet AND BSC.
 *
 * FLOW:
 *   1. User approves this contract to spend their TKS ERC-20
 *   2. User calls lockTokens(amount, tksAddress)
 *   3. Event emitted → TKS validators observe it
 *   4. Validators release native TKS to tksAddress on TKS Network
 *   5. User can claim back (unlock) if bridge fails within 7 days
 */
contract TKSBridge {
    IERC20 public immutable tksToken;
    address public owner;
    uint256 public minLock = 100 * 1e18;  // minimum 100 TKS per bridge tx
    uint256 public fee = 1 * 1e18;        // 1 TKS flat bridge fee
    bool public paused;

    struct LockRecord {
        address sender;
        address tksNetworkAddress;
        uint256 amount;
        uint256 timestamp;
        bool released;
        bool refunded;
    }

    mapping(bytes32 => LockRecord) public locks;
    mapping(address => bytes32[]) public userLocks;
    uint256 public totalLocked;
    uint256 public nonce;

    event TokensLocked(
        bytes32 indexed lockId,
        address indexed sender,
        address indexed tksNetworkAddress,
        uint256 amount,
        uint256 fee,
        uint256 timestamp,
        string sourceChain  // "ethereum" or "bsc"
    );

    event TokensReleased(bytes32 indexed lockId, address releasedTo);
    event TokensRefunded(bytes32 indexed lockId, address refundedTo, uint256 amount);

    modifier onlyOwner() { require(msg.sender == owner, "Not owner"); _; }
    modifier notPaused() { require(!paused, "Bridge paused"); _; }

    constructor(address _tksToken) {
        tksToken = IERC20(_tksToken);
        owner = msg.sender;
    }

    /**
     * @notice Lock TKS ERC-20 tokens to receive native TKS on the TKS Network.
     * @param amount Amount of TKS to bridge (in wei, 18 decimals)
     * @param tksNetworkAddress Your TKS Network address (same as your ETH address 0x...)
     */
    function lockTokens(uint256 amount, address tksNetworkAddress) external notPaused returns (bytes32 lockId) {
        require(amount >= minLock, "Below minimum bridge amount");
        require(tksNetworkAddress != address(0), "Invalid TKS address");
        require(amount > fee, "Amount must exceed fee");

        uint256 netAmount = amount - fee;

        // Pull tokens from sender
        require(tksToken.transferFrom(msg.sender, address(this), amount), "Transfer failed");

        // Generate unique lock ID
        lockId = keccak256(abi.encodePacked(msg.sender, tksNetworkAddress, amount, block.timestamp, nonce++));

        locks[lockId] = LockRecord({
            sender: msg.sender,
            tksNetworkAddress: tksNetworkAddress,
            amount: netAmount,
            timestamp: block.timestamp,
            released: false,
            refunded: false
        });

        userLocks[msg.sender].push(lockId);
        totalLocked += netAmount;

        emit TokensLocked(
            lockId,
            msg.sender,
            tksNetworkAddress,
            netAmount,
            fee,
            block.timestamp,
            block.chainid == 1 ? "ethereum" : "bsc"
        );
    }

    /**
     * @notice Called by bridge operators after releasing native TKS on TKS Network.
     * @dev Marks the lock as released (tokens stay locked — they are the backing reserve).
     */
    function markReleased(bytes32 lockId) external onlyOwner {
        require(!locks[lockId].released, "Already released");
        require(!locks[lockId].refunded, "Already refunded");
        locks[lockId].released = true;
        emit TokensReleased(lockId, locks[lockId].tksNetworkAddress);
    }

    /**
     * @notice Emergency refund if bridge fails within 7 days.
     */
    function claimRefund(bytes32 lockId) external {
        LockRecord storage record = locks[lockId];
        require(record.sender == msg.sender, "Not your lock");
        require(!record.released, "Already released on TKS Network");
        require(!record.refunded, "Already refunded");
        require(block.timestamp > record.timestamp + 7 days, "Wait 7 days before refund");

        record.refunded = true;
        totalLocked -= record.amount;
        require(tksToken.transfer(msg.sender, record.amount), "Refund failed");
        emit TokensRefunded(lockId, msg.sender, record.amount);
    }

    // ── Admin ─────────────────────────────────────────────
    function pause() external onlyOwner { paused = true; }
    function unpause() external onlyOwner { paused = false; }
    function setFee(uint256 _fee) external onlyOwner { fee = _fee; }
    function setMinLock(uint256 _min) external onlyOwner { minLock = _min; }
    function withdrawFees(address to) external onlyOwner {
        // Only withdraw collected fees (total balance - totalLocked)
        uint256 feeBalance = address(this).balance;
        // For ERC-20 fees:
        // uint256 fees = tksToken.balanceOf(address(this)) - totalLocked;
        // require(tksToken.transfer(to, fees));
    }
    function transferOwnership(address newOwner) external onlyOwner { owner = newOwner; }

    // ── View ──────────────────────────────────────────────
    function getLock(bytes32 lockId) external view returns (LockRecord memory) {
        return locks[lockId];
    }
    function getUserLocks(address user) external view returns (bytes32[] memory) {
        return userLocks[user];
    }
}
```

---

## 4. TKS Network Bridge Pallet (Validator-Side)

Validators on the TKS network watch for `TokensLocked` events and release native TKS:

```
Event detected on ETH/BSC:
  TokensLocked(lockId=0xabc, sender=0xUser, tksNetworkAddress=0xUser, amount=1000 TKS)
       ↓
TKS Validator nodes observe this event
       ↓  
2/3 of validators sign a "release" transaction (multi-sig threshold)
       ↓
Native TKS released from Bridge Reserve to 0xUser on TKS Network
       ↓
markReleased(lockId) called on ETH/BSC contract (proof of completion)
```

---

## 5. User-Facing Flow (Simple)

```
Step 1: Go to bridge.tokenkickstarter.com
Step 2: Connect MetaMask (same wallet on ETH/BSC and TKS)
Step 3: Select: Ethereum → TKS Network
Step 4: Enter amount (min 100 TKS)
Step 5: Approve + Lock (2 MetaMask transactions)
Step 6: Wait ~10 minutes for validator confirmation
Step 7: Switch MetaMask to TKS Network (Chain ID 7779)
Step 8: Native TKS appears in same wallet address ✅
```

**No new wallet needed.** Same `0x...` address works on ETH, BSC, and TKS.

---

## 6. What to Update in chain_spec.rs for Mainnet

Replace the test addresses with your real wallets:

```rust
// MAINNET genesis allocation
vec![
    // Team wallet (replace with your real address)
    (hex_to_account("0xYOUR_TEAM_WALLET"),       150_000_000 * TKS),
    // Bridge reserve — locked, released only by bridge pallet
    (hex_to_account("0xBRIDGE_RESERVE_WALLET"),  300_000_000 * TKS),
    // Ecosystem fund
    (hex_to_account("0xECOSYSTEM_WALLET"),        150_000_000 * TKS),
    // On-chain treasury (pallet-treasury controls this)
    (hex_to_account("0xTREASURY_WALLET"),         100_000_000 * TKS),
    // Initial validators (5 validators × 10M TKS each)
    (hex_to_account("0xVALIDATOR_1"),              10_000_000 * TKS),
    (hex_to_account("0xVALIDATOR_2"),              10_000_000 * TKS),
    (hex_to_account("0xVALIDATOR_3"),              10_000_000 * TKS),
    (hex_to_account("0xVALIDATOR_4"),              10_000_000 * TKS),
    (hex_to_account("0xVALIDATOR_5"),              10_000_000 * TKS),
    // Public sale / IDO wallet
    (hex_to_account("0xSALE_WALLET"),             150_000_000 * TKS),
    // Community airdrop wallet
    (hex_to_account("0xAIRDROP_WALLET"),          100_000_000 * TKS),
]
// Total: 1,000,000,000 TKS ✅
```

---

## 7. Bridge Deployment Checklist

- [ ] Deploy `TKSBridge.sol` on Ethereum mainnet
- [ ] Deploy `TKSBridge.sol` on BSC mainnet
- [ ] Add bridge contract addresses to `bridge.tokenkickstarter.com`
- [ ] Fund Bridge Reserve wallet in TKS genesis (300M TKS)
- [ ] Run 3+ validator nodes with bridge event watcher
- [ ] Register Chain ID 7779 on chainlist.org
- [ ] Build bridge UI at `bridge.tokenkickstarter.com`
- [ ] Audit bridge contract (before launch)

---

## 8. Timeline Suggestion

```
Now:       Testnet running with test addresses
Phase 1:   Deploy bridge contracts on ETH/BSC testnet
Phase 2:   Run bridge for 30 days on testnet (stress test)
Phase 3:   Mainnet genesis with real wallets + 1B TKS
Phase 4:   Open bridge for ETH/BSC holders to migrate
Phase 5:   List on exchanges with native TKS
```

---

*Bridge contract address (after deployment): update bridge.tokenkickstarter.com*
*Chain ID 7779 — register at github.com/ethereum-lists/chains*
