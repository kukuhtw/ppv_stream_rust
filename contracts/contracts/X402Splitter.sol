// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

interface IERC20Metadata {
  function transferFrom(address from, address to, uint256 amount) external returns (bool);
  function decimals() external view returns (uint8);
  function balanceOf(address account) external view returns (uint256);
}

contract X402Splitter is ReentrancyGuard {
  using ECDSA for bytes32;
  using SafeERC20 for IERC20Metadata;

  event Paid(
    bytes32 indexed invoiceUid,
    address indexed payer,
    address indexed creator,
    address admin,
    address token,       // address(0) untuk native
    uint256 amountWei,   // jumlah total dibayar (wei / smallest unit)
    string  videoId
  );

  address public immutable admin;        // admin platform (penandatangan)
  uint16  public constant BP_DENOM = 10000;

  // Cegah replay / duplikasi invoice:
  mapping(bytes32 => bool) public usedInvoice;

  constructor(address _admin) {
    require(_admin != address(0), "admin=0");
    admin = _admin;
  }

  // ----------------------------------------------------------------
  // VERIFIKASI TTD ADMIN
  // Admin menandatangani hash dari:
  // (invoiceUid, token, minAmountWei, creator, creatorBp, videoIdHash, payer, deadline, address(this), chainid)
  // ----------------------------------------------------------------
  function _verify(
    bytes32 invoiceUid,
    address token,
    uint256 minAmountWei,
    address creator,
    uint16  creatorBp,
    string  memory videoId,
    address payer,
    uint256 deadline,
    uint8 v, bytes32 r, bytes32 s
  ) internal view {
    require(block.timestamp <= deadline, "expired");
    bytes32 dataHash = keccak256(
      abi.encode(
        invoiceUid,
        token,
        minAmountWei,
        creator,
        creatorBp,
        keccak256(bytes(videoId)),
        payer,
        deadline,
        address(this),         // bind ke kontrak ini
        block.chainid          // bind ke chain ini
      )
    );
    address signer = ECDSA.toEthSignedMessageHash(dataHash).recover(v, r, s);
    require(signer == admin, "bad sig");
  }

  // ----------------------------
  // Native coin (ETH/MATIC/BNB)
  // Menolak underpaid:
  // - msg.value harus >= minAmountWei (dari TTD admin)
  // - invoiceUid hanya bisa dipakai 1x
  // ----------------------------
  function payNativeSigned(
    bytes32 invoiceUid,
    address creator,
    uint16  creatorBp,       // mis. 9000 = 90.00%
    string  calldata videoId,
    uint256 minAmountWei,    // nilai minimum yang disetujui admin
    uint256 deadline,
    uint8 v, bytes32 r, bytes32 s
  ) external payable nonReentrant {
    require(!usedInvoice[invoiceUid], "invoice used");
    require(msg.value > 0, "no value");
    require(creator != address(0), "creator=0");
    require(creatorBp <= BP_DENOM, "bp>100%");
    // Jika ingin exact: require(msg.value == minAmountWei, "amount mismatch");
    require(msg.value >= minAmountWei, "underpaid");

    _verify(
      invoiceUid,
      address(0),
      minAmountWei,
      creator,
      creatorBp,
      videoId,
      msg.sender,
      deadline,
      v, r, s
    );

    usedInvoice[invoiceUid] = true;

    uint256 toCreator = (msg.value * creatorBp) / BP_DENOM;
    uint256 toAdmin   = msg.value - toCreator;

    (bool ok1, ) = payable(creator).call{value: toCreator}("");
    require(ok1, "pay creator");
    (bool ok2, ) = payable(admin).call{value: toAdmin}("");
    require(ok2, "pay admin");

    emit Paid(invoiceUid, msg.sender, creator, admin, address(0), msg.value, videoId);
  }

  // ----------------------------
  // ERC-20
  // Menolak underpaid:
  // - amountWei yang ditarik harus >= minAmountWei (ditandatangani admin)
  // - pastikan user sudah approve amountWei ke kontrak
  // - invoiceUid hanya bisa dipakai 1x
  // ----------------------------
  function payERC20Signed(
    bytes32 invoiceUid,
    address token,
    uint256 amountWei,       // jumlah yang akan ditarik dari payer
    address creator,
    uint16  creatorBp,
    string  calldata videoId,
    uint256 minAmountWei,    // minimum yang disetujui admin
    uint256 deadline,
    uint8 v, bytes32 r, bytes32 s
  ) external nonReentrant {
    require(!usedInvoice[invoiceUid], "invoice used");
    require(amountWei > 0, "no amount");
    require(token != address(0), "token=0");
    require(creator != address(0), "creator=0");
    require(creatorBp <= BP_DENOM, "bp>100%");
    // Jika ingin exact: require(amountWei == minAmountWei, "amount mismatch");
    require(amountWei >= minAmountWei, "underpaid");

    _verify(
      invoiceUid,
      token,
      minAmountWei,
      creator,
      creatorBp,
      videoId,
      msg.sender,
      deadline,
      v, r, s
    );

    usedInvoice[invoiceUid] = true;

    uint256 toCreator = (amountWei * creatorBp) / BP_DENOM;
    uint256 toAdmin   = amountWei - toCreator;

    IERC20Metadata erc = IERC20Metadata(token);
    erc.safeTransferFrom(msg.sender, creator, toCreator);
    erc.safeTransferFrom(msg.sender, admin,   toAdmin);

    emit Paid(invoiceUid, msg.sender, creator, admin, token, amountWei, videoId);
  }
}
