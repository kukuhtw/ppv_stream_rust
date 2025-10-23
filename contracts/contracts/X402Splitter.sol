// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IERC20 {
  function transferFrom(address from, address to, uint256 amount) external returns (bool);
  function decimals() external view returns (uint8);
}

contract X402Splitter {
  event Paid(
    bytes32 indexed invoiceUid,
    address indexed payer,
    address indexed creator,
    address admin,
    address token,       // address(0) untuk native
    uint256 amountWei,   // nilai total yang dibayar (wei / smallest unit)
    string  videoId
  );

  address public immutable admin; // admin platform
  uint16  public constant BP_DENOM = 10000;

  constructor(address _admin) {
    require(_admin != address(0), "admin");
    admin = _admin;
  }

  // Native coin (ETH/MATIC/BNB)
  function payNative(
    bytes32 invoiceUid,
    address creator,
    uint16 creatorBp,    // mis. 9000 = 90.00%
    string calldata videoId
  ) external payable {
    require(msg.value > 0, "no value");
    uint256 toCreator = (msg.value * creatorBp) / BP_DENOM;
    uint256 toAdmin   = msg.value - toCreator;
    payable(creator).transfer(toCreator);
    payable(admin).transfer(toAdmin);
    emit Paid(invoiceUid, msg.sender, creator, admin, address(0), msg.value, videoId);
  }

  // ERC-20
  function payERC20(
    bytes32 invoiceUid,
    address token,
    uint256 amountWei,
    address creator,
    uint16 creatorBp,
    string calldata videoId
  ) external {
    require(amountWei > 0, "no amount");
    uint256 toCreator = (amountWei * creatorBp) / BP_DENOM;
    uint256 toAdmin   = amountWei - toCreator;

    require(IERC20(token).transferFrom(msg.sender, creator, toCreator), "tfc");
    require(IERC20(token).transferFrom(msg.sender, admin,   toAdmin),   "tfa");
    emit Paid(invoiceUid, msg.sender, creator, admin, token, amountWei, videoId);
  }
}
