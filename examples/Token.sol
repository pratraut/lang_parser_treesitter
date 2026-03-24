// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./Ownable.sol";
import "./IERC20.sol";

contract Token is Ownable {
    mapping(address => uint256) private _balances;
    mapping(address => mapping(address => uint256)) private _allowances;
    uint256 private _totalSupply;
    string  private _name;
    string  private _symbol;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    constructor(string memory name_, string memory symbol_, uint256 initialSupply) {
        _name   = name_;
        _symbol = symbol_;
        _mint(msg.sender, initialSupply);
    }

    modifier nonZeroAddress(address addr) {
        require(addr != address(0), "Token: zero address");
        _;
    }

    modifier sufficientBalance(address addr, uint256 amount) {
        require(_balances[addr] >= amount, "Token: insufficient balance");
        _;
    }

    function name() public view returns (string memory) {
        return _name;
    }

    function symbol() public view returns (string memory) {
        return _symbol;
    }

    function totalSupply() public view returns (uint256) {
        return _totalSupply;
    }

    function balanceOf(address account) public view returns (uint256) {
        return _balances[account];
    }

    function transfer(address to, uint256 amount)
        public
        nonZeroAddress(to)
        sufficientBalance(msg.sender, amount)
        returns (bool)
    {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function allowance(address owner, address spender) public view returns (uint256) {
        return _allowances[owner][spender];
    }

    function approve(address spender, uint256 amount)
        public
        nonZeroAddress(spender)
        returns (bool)
    {
        _approve(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount)
        public
        nonZeroAddress(from)
        nonZeroAddress(to)
        sufficientBalance(from, amount)
        returns (bool)
    {
        uint256 currentAllowance = _allowances[from][msg.sender];
        require(currentAllowance >= amount, "Token: insufficient allowance");
        _approve(from, msg.sender, currentAllowance - amount);
        _transfer(from, to, amount);
        return true;
    }

    function mint(address to, uint256 amount) public onlyOwner nonZeroAddress(to) {
        _mint(to, amount);
    }

    function burn(uint256 amount) public sufficientBalance(msg.sender, amount) {
        _burn(msg.sender, amount);
    }

    function _transfer(address from, address to, uint256 amount) internal {
        _balances[from] -= amount;
        _balances[to]   += amount;
        emit Transfer(from, to, amount);
    }

    function _mint(address account, uint256 amount) internal {
        _totalSupply        += amount;
        _balances[account]  += amount;
        emit Transfer(address(0), account, amount);
    }

    function _burn(address account, uint256 amount) internal {
        _totalSupply        -= amount;
        _balances[account]  -= amount;
        emit Transfer(account, address(0), amount);
    }

    function _approve(address owner, address spender, uint256 amount) internal {
        _allowances[owner][spender] = amount;
        emit Approval(owner, spender, amount);
    }
}
