/**
 * 正确的 CLMM 池价格获取方法
 *
 * CLMM 池不能直接用储备量计算价格！
 * 必须使用 sqrtPriceX64 字段
 */

const { Connection, PublicKey } = require('@solana/web3.js');

const SOL_USDT_POOL = '3nMFwZXwY1s1M5s8vYAHqd4wGs4iSxXE4LRoUMMYqEgF';
const WRAPPED_SOL_MINT = 'So11111111111111111111111111111111111111112';
const USDT_MINT = 'Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB';

function extractPublicKey(data, offset) {
  return new PublicKey(data.slice(offset, offset + 32));
}

/**
 * 读取 u128 (16 bytes) 小端序
 */
function readU128LE(buffer, offset) {
  let result = BigInt(0);
  for (let i = 0; i < 16; i++) {
    result += BigInt(buffer[offset + i]) << BigInt(8 * i);
  }
  return result;
}

/**
 * 从 sqrtPriceX64 计算实际价格
 *
 * sqrtPriceX64 = sqrt(price) * 2^64
 * price = (sqrtPriceX64 / 2^64)^2
 * price = amount_token_1 / amount_token_0
 */
function calculatePriceFromSqrtPriceX64(sqrtPriceX64, decimals0, decimals1) {
  // sqrtPriceX64 是 Q64.64 格式
  const Q64 = BigInt(2) ** BigInt(64);

  // 转换为浮点数
  const sqrtPrice = Number(sqrtPriceX64) / Number(Q64);

  // price = sqrtPrice^2
  let price = sqrtPrice * sqrtPrice;

  // 调整小数位差异
  const decimalAdjustment = Math.pow(10, decimals0 - decimals1);
  price = price * decimalAdjustment;

  return price;
}

async function getCorrectClmmPrice() {
  console.log('=========================================');
  console.log('  CLMM 池正确价格获取方法');
  console.log('=========================================\n');

  const connection = new Connection(
    'https://mainnet.helius-rpc.com/?api-key=666f279b-0b08-41cd-97f4-461811d7fc7a',
    'confirmed'
  );

  console.log('[1/4] 读取池账户');
  const poolPubkey = new PublicKey(SOL_USDT_POOL);
  const poolAccount = await connection.getAccountInfo(poolPubkey);

  if (!poolAccount) {
    throw new Error('无法读取池账户');
  }
  console.log('      ✓ 完成\n');

  console.log('[2/4] 解析账户结构');
  const mint0 = extractPublicKey(poolAccount.data, 73);
  const mint1 = extractPublicKey(poolAccount.data, 105);
  const decimals0 = poolAccount.data[233];
  const decimals1 = poolAccount.data[234];

  // 读取 sqrtPriceX64 (offset 253, u128)
  const sqrtPriceX64 = readU128LE(poolAccount.data, 253);

  console.log(`      Mint 0: ${mint0.toBase58()}`);
  console.log(`      Mint 1: ${mint1.toBase58()}`);
  console.log(`      Decimals: ${decimals0}, ${decimals1}`);
  console.log(`      sqrtPriceX64: ${sqrtPriceX64.toString()}`);
  console.log('      ✓ 完成\n');

  console.log('[3/4] 计算价格');

  // 计算价格：token1/token0
  let price = calculatePriceFromSqrtPriceX64(sqrtPriceX64, decimals0, decimals1);

  console.log(`      原始价格 (Token1/Token0): ${price.toFixed(6)}`);

  // 确定哪个是 SOL，哪个是 USDT
  let solPrice;
  if (mint0.toBase58() === WRAPPED_SOL_MINT && mint1.toBase58() === USDT_MINT) {
    // price 已经是 USDT/SOL
    solPrice = price;
    console.log(`      识别: SOL 是 Token0, USDT 是 Token1`);
  } else if (mint1.toBase58() === WRAPPED_SOL_MINT && mint0.toBase58() === USDT_MINT) {
    // price 是 SOL/USDT，需要倒数
    solPrice = 1 / price;
    console.log(`      识别: USDT 是 Token0, SOL 是 Token1`);
  } else {
    throw new Error('无法识别 SOL/USDT');
  }

  console.log('      ✓ 完成\n');

  console.log('[4/4] 获取储备量（用于流动性计算）');
  const vault0 = extractPublicKey(poolAccount.data, 137);
  const vault1 = extractPublicKey(poolAccount.data, 169);

  const vault0Balance = await connection.getTokenAccountBalance(vault0);
  const vault1Balance = await connection.getTokenAccountBalance(vault1);

  const amount0 = parseFloat(vault0Balance.value.uiAmount);
  const amount1 = parseFloat(vault1Balance.value.uiAmount);

  console.log(`      SOL 储备: ${amount0.toFixed(2)}`);
  console.log(`      USDT 储备: ${amount1.toFixed(2)}`);
  console.log('      ✓ 完成\n');

  // 输出结果
  console.log('=========================================');
  console.log('  价格信息（正确方法）');
  console.log('=========================================');
  console.log(`  💰 SOL 价格:  $${solPrice.toFixed(4)} USDT`);
  console.log(`  `);
  console.log(`  SOL 储备:    ${amount0.toLocaleString()} SOL`);
  console.log(`  USDT 储备:   ${amount1.toLocaleString()} USDT`);
  console.log(`  流动性:      $${(solPrice * amount0 * 2).toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  })}`);
  console.log('=========================================\n');

  console.log('⚠ 注意：CLMM 池使用集中流动性，储备量不能直接用于计算价格！');
  console.log('价格必须从 sqrtPriceX64 字段计算。\n');

  return {
    price: solPrice,
    solReserve: amount0,
    usdtReserve: amount1,
    tvl: solPrice * amount0 * 2,
    poolAddress: SOL_USDT_POOL,
    sqrtPriceX64: sqrtPriceX64.toString(),
  };
}

if (require.main === module) {
  getCorrectClmmPrice()
    .then(result => {
      console.log('返回数据:');
      console.log(JSON.stringify(result, null, 2));
      console.log('\n✓ 执行完成');
    })
    .catch(error => {
      console.error('\n✗ 错误:', error.message);
      console.error(error.stack);
      process.exit(1);
    });
}

module.exports = { getCorrectClmmPrice };
