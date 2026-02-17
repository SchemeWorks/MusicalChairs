# Emoji-to-Icon Mapping Reference

This file preserves the original emoji usage for potential revert.

## Mapping Table

| Emoji | Lucide Icon | Context | Files |
|-------|-------------|---------|-------|
| `🎰` | `Dices` | Slot machine / gambling theme | App.tsx, ProfileSetup.tsx, GameTracking.tsx, GamePlans.tsx |
| `⚠️` | `AlertTriangle` | Warning / danger indicators | App.tsx, ProfileSetup.tsx, AddHouseMoney.tsx, GamePlans.tsx, HouseDashboard.tsx |
| `🎲` | `Dice5` | Dice / gambling / random | Dashboard.tsx, HouseDashboard.tsx, Shenanigans.tsx |
| `💥` | `Zap` | Explosion / error | ErrorBoundary.tsx, Shenanigans.tsx (outcome) |
| `🎭` | `Drama` (not in lucide) → `Theater` | Theater masks / profile | App.tsx |
| `🔧` | `Wrench` | Admin / tools | App.tsx |
| `🎪` | `Tent` | Circus / dashboard | App.tsx |
| `🥇` | `Medal` (gold-styled) | Rank 1 | HallOfFame.tsx |
| `🥈` | `Medal` (silver-styled) | Rank 2 | HallOfFame.tsx |
| `🥉` | `Medal` (bronze-styled) | Rank 3 | HallOfFame.tsx |
| `🏅` | `Award` | Rank 4+ | HallOfFame.tsx |
| `🏆` | `Trophy` | Trophy / hall of fame | HallOfFame.tsx, Dashboard.tsx |
| `🎯` | `Target` | Target / aim | HallOfFame.tsx |
| `🚀` | `Rocket` | Launch / YOLO | GameTracking.tsx, GamePlans.tsx |
| `🔐` | SVG `/ii-logo.svg` | Internet Identity wallet | WalletDropdown.tsx |
| `🔌` | SVG `/plug-logo.svg` | Plug wallet | WalletDropdown.tsx |
| `✨` | SVG `/oisy-logo.svg` or `Sparkles` | OISY wallet / sparkle | WalletDropdown.tsx, Shenanigans.tsx |
| `💳` | `CreditCard` | Default wallet | WalletDropdown.tsx |
| `✏️` | `Pencil` | Edit / rename | WalletDropdown.tsx, Shenanigans.tsx |
| `📊` | `BarChart3` | Charts / data | HouseDashboard.tsx, GamePlans.tsx |
| `🔥` | `Flame` | Fire / compounding | HouseDashboard.tsx, GamePlans.tsx |
| `💰` | `Coins` | Money / incoming | HouseDashboard.tsx, Shenanigans.tsx |
| `💸` | `Banknote` | Money outgoing | HouseDashboard.tsx |
| `💎` | `Gem` | Diamond / premium | HouseDashboard.tsx, GamePlans.tsx |
| `🌱` | `Sprout` | Simple mode / growth | GamePlans.tsx |
| `🏰` | `Castle` → `Landmark` | Castle / dealers | HouseDashboard.tsx |
| `🏦` | `Building2` | Bank | Shenanigans.tsx |
| `🌊` | `Waves` | Wave | Shenanigans.tsx |
| `🪞` | `FlipHorizontal2` | Mirror | Shenanigans.tsx |
| `⬆️` | `ArrowUp` | Up arrow | Shenanigans.tsx |
| `✂️` | `Scissors` | Cut | Shenanigans.tsx |
| `🐋` | `Fish` | Whale | Shenanigans.tsx |
| `📈` | `TrendingUp` | Chart uptrend | Shenanigans.tsx |
| `🎛️` | `SlidersHorizontal` | Control knobs / admin | ShenanigansAdminPanel.tsx |
| `😉` | (removed) | Toast emoji | GamePlans.tsx |
| `🔄` | `RefreshCw` | Loop / neutral outcome | Shenanigans.tsx |

## Shenanigan Icons (index-based)

```
0: '💰' → Coins
1: '🌊' → Waves
2: '✏️' → Pencil
3: '🏦' → Building2
4: '🎯' → Target
5: '🪞' → FlipHorizontal2
6: '⬆️' → ArrowUp
7: '✂️' → Scissors
8: '🐋' → Fish
9: '📈' → TrendingUp
10: '✨' → Sparkles
```
