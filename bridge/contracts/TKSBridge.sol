// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IERC20 {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

/**
 * @title TKSBridge
 * @author TokenKickstarter
 * @notice Lock TKS ERC-20 tokens here → receive native TKS on TKS Network (Chain ID 7779).
 *
 * Deploy on:
 *   - Ethereum mainnet (TKS ERC-20: 0x...)
 *   - BSC mainnet     (TKS BEP-20: 0x...)
 *
 * BRIDGE FLOW:
 *   1. User: approve(bridgeAddress, amount)
 *   2. User: lockTokens(amount, tksNetworkAddress)
 *   3. Event emitted on-chain
 *   4. TKS validators observe TokensLocked event
 *   5. 2/3 validators sign → native TKS released on TKS Network
 *   6. Bridge operator calls markReleased(lockId)
 *
 * SAME ADDRESS: TKS Network uses Ethereum addresses (0x...),
 * so the same MetaMask wallet receives native TKS automatically.
 *
 * REFUND: If bridge fails, user can claimRefund() after 7 days.
 *
 * FEES: Flat 1 TKS fee per bridge transaction (configurable by owner).
 */
contract TKSBridge {

    // ── Constants ──────────────────────────────────────────────────────
    /// Canonical EVM burn address — tokens sent here are permanently destroyed.
    address public constant DEAD_ADDRESS = 0x000000000000000000000000000000000000dEaD;

    // ── State ──────────────────────────────────────────────────────────
    IERC20 public immutable tksToken;
    address public owner;
    uint256 public minLock    = 100 * 1e18;   // minimum 100 TKS per bridge tx
    uint256 public fee        = 1   * 1e18;   // 1 TKS flat bridge fee
    uint256 public nonce;
    uint256 public totalLocked;   // tokens locked (backing reserve)
    uint256 public totalBurned;   // tokens permanently burned via burnAndBridge()
    bool    public paused;

    struct LockRecord {
        address sender;             // who locked/burned tokens
        address tksNetworkAddress;  // who receives on TKS Network
        uint256 amount;             // net amount (after fee)
        uint256 timestamp;          // when locked/burned
        bool    released;           // confirmed released on TKS Network
        bool    refunded;           // emergency refunded
        bool    burned;             // true = permanently burned, no refund
    }

    mapping(bytes32 => LockRecord) public locks;
    mapping(address => bytes32[])  public userLocks;

    // ── Events ─────────────────────────────────────────────────────────
    event TokensLocked(
        bytes32 indexed lockId,
        address indexed sender,
        address indexed tksNetworkAddress,
        uint256 amount,
        uint256 fee,
        uint256 timestamp,
        uint256 chainId
    );
    /// @notice Emitted when tokens are permanently burned via burnAndBridge()
    event TokensBurned(
        bytes32 indexed burnId,
        address indexed sender,
        address indexed tksNetworkAddress,
        uint256 amount,
        uint256 fee,
        uint256 timestamp,
        uint256 chainId
    );
    event TokensReleased(bytes32 indexed lockId, address releasedTo, uint256 amount);
    event TokensRefunded(bytes32 indexed lockId, address refundedTo, uint256 amount);
    event FeeUpdated(uint256 oldFee, uint256 newFee);
    event MinLockUpdated(uint256 oldMin, uint256 newMin);
    event Paused(address by);
    event Unpaused(address by);

    // ── Modifiers ──────────────────────────────────────────────────────
    modifier onlyOwner() {
        require(msg.sender == owner, "TKSBridge: not owner");
        _;
    }
    modifier notPaused() {
        require(!paused, "TKSBridge: bridge is paused");
        _;
    }

    // ── Constructor ────────────────────────────────────────────────────
    constructor(address _tksToken) {
        require(_tksToken != address(0), "TKSBridge: zero token address");
        tksToken = IERC20(_tksToken);
        owner = msg.sender;
    }

    // ── Core: Lock ─────────────────────────────────────────────────────

    /**
     * @notice Lock TKS ERC-20 tokens to receive native TKS on the TKS Network.
     * @dev Tokens are locked (not burned). Use burnAndBridge() to permanently
     *      reduce ETH/BSC supply. Locked tokens remain as backing reserve.
     * @param amount          Total TKS to lock (in wei). Net = amount - fee.
     * @param tksNetworkAddress  Your address on TKS Network (same as ETH 0x address).
     * @return lockId          Unique ID for this lock — save it to track status.
     */
    function lockTokens(uint256 amount, address tksNetworkAddress)
        external
        notPaused
        returns (bytes32 lockId)
    {
        require(amount >= minLock,              "TKSBridge: below minimum lock amount");
        require(amount > fee,                   "TKSBridge: amount must exceed fee");
        require(tksNetworkAddress != address(0),"TKSBridge: invalid TKS Network address");

        uint256 netAmount = amount - fee;

        bool ok = tksToken.transferFrom(msg.sender, address(this), amount);
        require(ok, "TKSBridge: token transfer failed");

        lockId = keccak256(abi.encodePacked(
            msg.sender, tksNetworkAddress, amount, block.timestamp, block.chainid, nonce++
        ));

        locks[lockId] = LockRecord({
            sender:             msg.sender,
            tksNetworkAddress:  tksNetworkAddress,
            amount:             netAmount,
            timestamp:          block.timestamp,
            released:           false,
            refunded:           false,
            burned:             false
        });

        userLocks[msg.sender].push(lockId);
        totalLocked += netAmount;

        emit TokensLocked(
            lockId, msg.sender, tksNetworkAddress,
            netAmount, fee, block.timestamp, block.chainid
        );
    }

    /**
     * @notice BURN TKS ERC-20 and receive native TKS on TKS Network.
     *
     * Unlike lockTokens(), this PERMANENTLY DESTROYS the ERC-20 tokens by
     * sending them to the dead address (0x000...dEaD). This reduces the total
     * ETH/BSC TKS supply forever, making native TKS more scarce.
     *
     * The 1B TKS currently on ETH/BSC is gradually eliminated as holders
     * migrate via this function. Remaining native TKS circulates only on
     * the TKS Network (Chain ID 7779).
     *
     * @param amount          Total TKS to burn (in wei). Net = amount - fee.
     * @param tksNetworkAddress  Your address on TKS Network to receive native TKS.
     * @return burnId          Unique ID for this burn — save it to track status.
     */
    function burnAndBridge(uint256 amount, address tksNetworkAddress)
        external
        notPaused
        returns (bytes32 burnId)
    {
        require(amount >= minLock,              "TKSBridge: below minimum amount");
        require(amount > fee,                   "TKSBridge: amount must exceed fee");
        require(tksNetworkAddress != address(0),"TKSBridge: invalid TKS Network address");

        uint256 netAmount = amount - fee;

        // Pull tokens from sender
        bool ok = tksToken.transferFrom(msg.sender, address(this), amount);
        require(ok, "TKSBridge: token transfer failed");

        // Burn: send net amount to dead address — PERMANENTLY DESTROYED
        bool burned = tksToken.transfer(DEAD_ADDRESS, netAmount);
        require(burned, "TKSBridge: burn transfer failed");

        // Fee stays in contract for withdrawal

        burnId = keccak256(abi.encodePacked(
            msg.sender, tksNetworkAddress, amount, block.timestamp, block.chainid, nonce++
        ));

        locks[burnId] = LockRecord({
            sender:             msg.sender,
            tksNetworkAddress:  tksNetworkAddress,
            amount:             netAmount,
            timestamp:          block.timestamp,
            released:           false,
            refunded:           false,
            burned:             true   // ← permanently burned, no refund possible
        });

        userLocks[msg.sender].push(burnId);
        totalBurned += netAmount;

        emit TokensBurned(
            burnId, msg.sender, tksNetworkAddress,
            netAmount, fee, block.timestamp, block.chainid
        );
    }


    // ── Core: Release Confirmation ──────────────────────────────────────

    /**
     * @notice Mark a lock as released after native TKS has been sent on TKS Network.
     * @dev Called by bridge operators after confirmed release on TKS Network.
     *      Locked tokens remain in contract as backing reserve.
     * @param lockId  The lock ID from the TokensLocked event.
     */
    function markReleased(bytes32 lockId) external onlyOwner {
        LockRecord storage rec = locks[lockId];
        require(rec.sender != address(0), "TKSBridge: lock not found");
        require(!rec.released,            "TKSBridge: already released");
        require(!rec.refunded,            "TKSBridge: already refunded");

        rec.released = true;
        emit TokensReleased(lockId, rec.tksNetworkAddress, rec.amount);
    }

    /**
     * @notice Batch mark multiple locks as released (gas efficient).
     */
    function markReleasedBatch(bytes32[] calldata lockIds) external onlyOwner {
        for (uint256 i = 0; i < lockIds.length; i++) {
            LockRecord storage rec = locks[lockIds[i]];
            if (!rec.released && !rec.refunded && rec.sender != address(0)) {
                rec.released = true;
                emit TokensReleased(lockIds[i], rec.tksNetworkAddress, rec.amount);
            }
        }
    }

    // ── Core: Emergency Refund ──────────────────────────────────────────

    /**
     * @notice Claim a refund if bridge has not released within 7 days.
     * @dev Anyone can trigger this for their own lock after the timeout.
     * @param lockId  The lock ID from the TokensLocked event.
     */
    function claimRefund(bytes32 lockId) external {
        LockRecord storage rec = locks[lockId];
        require(rec.sender == msg.sender,             "TKSBridge: not your lock");
        require(!rec.burned,                          "TKSBridge: burned locks cannot be refunded");
        require(!rec.released,                        "TKSBridge: already released on TKS Network");
        require(!rec.refunded,                        "TKSBridge: already refunded");
        require(block.timestamp >= rec.timestamp + 7 days, "TKSBridge: refund not available yet");

        rec.refunded   = true;
        totalLocked   -= rec.amount;

        bool ok = tksToken.transfer(msg.sender, rec.amount);
        require(ok, "TKSBridge: refund transfer failed");

        emit TokensRefunded(lockId, msg.sender, rec.amount);
    }

    // ── Admin ───────────────────────────────────────────────────────────

    function pause()   external onlyOwner { paused = true;  emit Paused(msg.sender); }
    function unpause() external onlyOwner { paused = false; emit Unpaused(msg.sender); }

    function setFee(uint256 _fee) external onlyOwner {
        require(_fee < minLock, "TKSBridge: fee cannot exceed minLock");
        emit FeeUpdated(fee, _fee);
        fee = _fee;
    }

    function setMinLock(uint256 _min) external onlyOwner {
        require(_min > fee, "TKSBridge: minLock must exceed fee");
        emit MinLockUpdated(minLock, _min);
        minLock = _min;
    }

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "TKSBridge: zero address");
        owner = newOwner;
    }

    /**
     * @notice Withdraw collected bridge fees (NOT the locked user tokens).
     * @dev Only withdraws tokens above the totalLocked amount.
     */
    function withdrawFees(address to) external onlyOwner {
        uint256 balance = tksToken.balanceOf(address(this));
        uint256 fees    = balance - totalLocked;
        require(fees > 0, "TKSBridge: no fees to withdraw");
        require(tksToken.transfer(to, fees), "TKSBridge: fee withdrawal failed");
    }

    // ── View ────────────────────────────────────────────────────────────

    function getLock(bytes32 lockId) external view returns (LockRecord memory) {
        return locks[lockId];
    }

    function getUserLocks(address user) external view returns (bytes32[] memory) {
        return userLocks[user];
    }

    function getUserLockCount(address user) external view returns (uint256) {
        return userLocks[user].length;
    }

    function getLockedBalance() external view returns (uint256) {
        return totalLocked;
    }

    function getTotalBurned() external view returns (uint256) {
        return totalBurned;
    }

    function getCollectedFees() external view returns (uint256) {
        return tksToken.balanceOf(address(this)) - totalLocked;
    }

    /**
     * @notice Summary of supply migration status.
     * @return locked    Tokens locked as backing reserve (bridgeable back if needed)
     * @return burned    Tokens permanently burned from ETH/BSC supply
     * @return fees      Fees collected by bridge
     */
    function getSupplyInfo() external view returns (
        uint256 locked,
        uint256 burned,
        uint256 fees
    ) {
        locked = totalLocked;
        burned = totalBurned;
        fees   = tksToken.balanceOf(address(this)) - totalLocked;
    }

    function isLockPending(bytes32 lockId) external view returns (bool) {
        LockRecord memory rec = locks[lockId];
        return rec.sender != address(0) && !rec.released && !rec.refunded && !rec.burned;
    }
}
