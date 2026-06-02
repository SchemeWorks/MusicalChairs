import ExitLiquidityGame from './exit-liquidity/ExitLiquidityGame';
import ExitLiquidityLeaderboard from './exit-liquidity/ExitLiquidityLeaderboard';

export default function ExitLiquidity() {
  return (
    <div className="py-2">
      <ExitLiquidityGame />
      <ExitLiquidityLeaderboard />
    </div>
  );
}
