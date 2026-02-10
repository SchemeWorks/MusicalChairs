import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Separator } from '@/components/ui/separator';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { toast } from 'sonner';
import { Save, RotateCcw, AlertTriangle, CheckCircle, Info } from 'lucide-react';

interface ShenaniganConfig {
  id: string;
  name: string;
  icon: string;
  cost: number;
  successOdds: number;
  failOdds: number;
  backfireOdds: number;
  minRange: number;
  maxRange: number;
  weightedRange: string;
  maxEffect: number;
  minTarget: number;
  cooldownGlobal: number;
  cooldownPerTarget: number;
  perRoundLimit: number;
  enabled: boolean;
  duration?: number;
  protectionThreshold?: number;
}

const defaultShenanigans: ShenaniganConfig[] = [
  {
    id: 'moneyTrickster',
    name: 'Money Trickster',
    icon: '💰',
    cost: 120,
    successOdds: 60,
    failOdds: 25,
    backfireOdds: 15,
    minRange: 2,
    maxRange: 8,
    weightedRange: '3-5%',
    maxEffect: 250,
    minTarget: 400,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 0,
    enabled: true,
  },
  {
    id: 'aoeSkim',
    name: 'AOE Skim',
    icon: '🌊',
    cost: 600,
    successOdds: 40,
    failOdds: 40,
    backfireOdds: 20,
    minRange: 1,
    maxRange: 3,
    weightedRange: '1%:70%, 2%:25%, 3%:5%',
    maxEffect: 60,
    minTarget: 0,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 1,
    enabled: true,
    protectionThreshold: 300,
  },
  {
    id: 'renameSpell',
    name: 'Rename Spell',
    icon: '✏️',
    cost: 200,
    successOdds: 90,
    failOdds: 5,
    backfireOdds: 5,
    minRange: 0,
    maxRange: 0,
    weightedRange: 'N/A',
    maxEffect: 0,
    minTarget: 0,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 0,
    enabled: true,
    duration: 7,
  },
  {
    id: 'mintTaxSiphon',
    name: 'Mint Tax Siphon',
    icon: '🏦',
    cost: 1200,
    successOdds: 70,
    failOdds: 20,
    backfireOdds: 10,
    minRange: 5,
    maxRange: 5,
    weightedRange: '5%',
    maxEffect: 1000,
    minTarget: 0,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 0,
    enabled: true,
    duration: 7,
  },
  {
    id: 'downlineHeist',
    name: 'Downline Heist',
    icon: '🎯',
    cost: 500,
    successOdds: 30,
    failOdds: 30,
    backfireOdds: 10,
    minRange: 0,
    maxRange: 0,
    weightedRange: 'L3:30/L2:20/L1:10',
    maxEffect: 0,
    minTarget: 0,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 1,
    enabled: true,
  },
  {
    id: 'magicMirror',
    name: 'Magic Mirror',
    icon: '🪞',
    cost: 200,
    successOdds: 100,
    failOdds: 0,
    backfireOdds: 0,
    minRange: 0,
    maxRange: 0,
    weightedRange: 'N/A',
    maxEffect: 2,
    minTarget: 0,
    cooldownGlobal: 0,
    cooldownPerTarget: 0,
    perRoundLimit: 0,
    enabled: true,
  },
  {
    id: 'ppBoosterAura',
    name: 'PP Booster Aura',
    icon: '⬆️',
    cost: 300,
    successOdds: 100,
    failOdds: 0,
    backfireOdds: 0,
    minRange: 5,
    maxRange: 15,
    weightedRange: '7-10%',
    maxEffect: 0,
    minTarget: 0,
    cooldownGlobal: 0,
    cooldownPerTarget: 0,
    perRoundLimit: 1,
    enabled: true,
  },
  {
    id: 'purseCutter',
    name: 'Purse Cutter',
    icon: '✂️',
    cost: 900,
    successOdds: 20,
    failOdds: 50,
    backfireOdds: 30,
    minRange: 25,
    maxRange: 50,
    weightedRange: '30-35%',
    maxEffect: 800,
    minTarget: 1000,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 0,
    enabled: true,
  },
  {
    id: 'whaleRebalance',
    name: 'Whale Rebalance',
    icon: '🐋',
    cost: 800,
    successOdds: 50,
    failOdds: 30,
    backfireOdds: 20,
    minRange: 20,
    maxRange: 20,
    weightedRange: '20%',
    maxEffect: 300,
    minTarget: 500,
    cooldownGlobal: 120,
    cooldownPerTarget: 180,
    perRoundLimit: 0,
    enabled: true,
  },
  {
    id: 'downlineBoost',
    name: 'Downline Boost',
    icon: '📈',
    cost: 400,
    successOdds: 100,
    failOdds: 0,
    backfireOdds: 0,
    minRange: 0,
    maxRange: 0,
    weightedRange: '1.3x',
    maxEffect: 0,
    minTarget: 0,
    cooldownGlobal: 0,
    cooldownPerTarget: 0,
    perRoundLimit: 1,
    enabled: true,
  },
  {
    id: 'goldenName',
    name: 'Golden Name',
    icon: '✨',
    cost: 100,
    successOdds: 100,
    failOdds: 0,
    backfireOdds: 0,
    minRange: 0,
    maxRange: 0,
    weightedRange: 'N/A',
    maxEffect: 0,
    minTarget: 0,
    cooldownGlobal: 0,
    cooldownPerTarget: 0,
    perRoundLimit: 1,
    enabled: true,
    duration: 1,
  },
];

export default function AdminDashboard() {
  const [shenanigans, setShenanigans] = useState<ShenaniganConfig[]>(defaultShenanigans);
  const [selectedShenanigan, setSelectedShenanigan] = useState<ShenaniganConfig | null>(null);
  const [globalSettings, setGlobalSettings] = useState({
    ppBudgetPerRound: 2000,
    mirrorStackCap: 2,
    lossProtectionThreshold: 200,
    cooldownProtectionHours: 24,
  });

  const handleSaveShenanigan = () => {
    if (!selectedShenanigan) return;

    // Validate odds sum to 100%
    const oddsSum = selectedShenanigan.successOdds + selectedShenanigan.failOdds + selectedShenanigan.backfireOdds;
    if (oddsSum !== 100) {
      toast.error('Odds must sum to 100%');
      return;
    }

    // Validate all numeric values are non-negative
    const numericFields = [
      'cost', 'successOdds', 'failOdds', 'backfireOdds', 'minRange', 'maxRange',
      'maxEffect', 'minTarget', 'cooldownGlobal', 'cooldownPerTarget', 'perRoundLimit'
    ];
    
    for (const field of numericFields) {
      if ((selectedShenanigan as any)[field] < 0) {
        toast.error(`${field} cannot be negative`);
        return;
      }
    }

    // Update shenanigans list
    setShenanigans(prev => 
      prev.map(s => s.id === selectedShenanigan.id ? selectedShenanigan : s)
    );

    toast.success('Shenanigan updated successfully');
  };

  const handleResetToDefaults = () => {
    setShenanigans(defaultShenanigans);
    setGlobalSettings({
      ppBudgetPerRound: 2000,
      mirrorStackCap: 2,
      lossProtectionThreshold: 200,
      cooldownProtectionHours: 24,
    });
    toast.success('All settings reset to defaults');
  };

  const handleSaveGlobalSettings = () => {
    // Validate all values are non-negative
    if (Object.values(globalSettings).some(v => v < 0)) {
      toast.error('All values must be non-negative');
      return;
    }

    toast.success('Global settings saved successfully');
  };

  return (
    <div className="space-y-6">
      <div className="text-center">
        <h2 className="text-2xl font-black text-white text-with-backdrop">
          🔧 Admin Dashboard 🔧
        </h2>
        <p className="text-center text-white text-with-backdrop mt-2">
          Configure shenanigans parameters and global settings
        </p>
      </div>

      <div className="rewards-single-container">
        {/* Minter Configuration Reminder Banner */}
        <Alert className="mb-6 bg-yellow-50 border-2 border-yellow-400">
          <Info className="h-5 w-5 text-yellow-600" />
          <AlertDescription className="text-yellow-900">
            <div className="font-bold text-lg mb-2">⚠️ IMPORTANT: Token Canister Minter Configuration</div>
            <div className="space-y-2 text-sm">
              <p>
                <strong>After each new live deployment, you MUST update the Ponzi Points token canister's minter:</strong>
              </p>
              <ol className="list-decimal list-inside space-y-1 ml-4">
                <li>Get your backend canister ID from the deployment output</li>
                <li>Run: <code className="bg-yellow-100 px-2 py-1 rounded">dfx canister call awsqm-4qaaa-aaaau-aclja-cai set_minter '(principal "YOUR_BACKEND_CANISTER_ID")'</code></li>
                <li>Verify the minter was set correctly</li>
              </ol>
              <p className="mt-2">
                <strong>Token Canister ID:</strong> <code className="bg-yellow-100 px-2 py-1 rounded">awsqm-4qaaa-aaaau-aclja-cai</code>
              </p>
              <p className="text-xs mt-2 text-yellow-700">
                Without this configuration, Ponzi Points minting will fail and the game will not function properly.
              </p>
            </div>
          </AlertDescription>
        </Alert>

        <Tabs defaultValue="shenanigans" className="w-full">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="shenanigans">Shenanigans Editor</TabsTrigger>
            <TabsTrigger value="global">Global Settings</TabsTrigger>
            <TabsTrigger value="analytics">Analytics</TabsTrigger>
          </TabsList>

          <TabsContent value="shenanigans" className="space-y-6">
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                All changes take effect immediately. Ensure odds sum to 100% and all values are non-negative.
                All Ponzi Points operations are handled through the ICRC-2 token canister.
              </AlertDescription>
            </Alert>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              {/* Shenanigans List */}
              <Card>
                <CardHeader>
                  <CardTitle>Available Shenanigans</CardTitle>
                  <CardDescription>Select a shenanigan to edit its parameters</CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    {shenanigans.map((shen) => (
                      <div
                        key={shen.id}
                        onClick={() => setSelectedShenanigan(shen)}
                        className={`p-3 rounded-lg border-2 cursor-pointer transition-all ${
                          selectedShenanigan?.id === shen.id
                            ? 'border-purple-500 bg-purple-50'
                            : 'border-gray-200 hover:border-purple-300'
                        }`}
                      >
                        <div className="flex items-center justify-between">
                          <div className="flex items-center space-x-3">
                            <span className="text-2xl">{shen.icon}</span>
                            <div>
                              <div className="font-bold text-gray-900">{shen.name}</div>
                              <div className="text-xs text-gray-600">Cost: {shen.cost} PP</div>
                            </div>
                          </div>
                          <Switch
                            checked={shen.enabled}
                            onCheckedChange={(checked) => {
                              setShenanigans(prev =>
                                prev.map(s => s.id === shen.id ? { ...s, enabled: checked } : s)
                              );
                            }}
                          />
                        </div>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>

              {/* Editor Panel */}
              {selectedShenanigan && (
                <Card>
                  <CardHeader>
                    <CardTitle className="flex items-center">
                      <span className="text-2xl mr-2">{selectedShenanigan.icon}</span>
                      {selectedShenanigan.name}
                    </CardTitle>
                    <CardDescription>Edit parameters for this shenanigan</CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-4">
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <Label>Cost (PP)</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.cost}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            cost: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                      <div>
                        <Label>Max Effect</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.maxEffect}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            maxEffect: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                    </div>

                    <Separator />

                    <div>
                      <Label className="mb-2 block">Odds (must sum to 100%)</Label>
                      <div className="grid grid-cols-3 gap-2">
                        <div>
                          <Label className="text-xs">Success %</Label>
                          <Input
                            type="number"
                            value={selectedShenanigan.successOdds}
                            onChange={(e) => setSelectedShenanigan({
                              ...selectedShenanigan,
                              successOdds: Math.max(0, Math.min(100, parseInt(e.target.value) || 0))
                            })}
                            min="0"
                            max="100"
                          />
                        </div>
                        <div>
                          <Label className="text-xs">Fail %</Label>
                          <Input
                            type="number"
                            value={selectedShenanigan.failOdds}
                            onChange={(e) => setSelectedShenanigan({
                              ...selectedShenanigan,
                              failOdds: Math.max(0, Math.min(100, parseInt(e.target.value) || 0))
                            })}
                            min="0"
                            max="100"
                          />
                        </div>
                        <div>
                          <Label className="text-xs">Backfire %</Label>
                          <Input
                            type="number"
                            value={selectedShenanigan.backfireOdds}
                            onChange={(e) => setSelectedShenanigan({
                              ...selectedShenanigan,
                              backfireOdds: Math.max(0, Math.min(100, parseInt(e.target.value) || 0))
                            })}
                            min="0"
                            max="100"
                          />
                        </div>
                      </div>
                      <div className="mt-2">
                        {selectedShenanigan.successOdds + selectedShenanigan.failOdds + selectedShenanigan.backfireOdds === 100 ? (
                          <Badge variant="outline" className="bg-green-50 text-green-700 border-green-300">
                            <CheckCircle className="h-3 w-3 mr-1" />
                            Valid (100%)
                          </Badge>
                        ) : (
                          <Badge variant="outline" className="bg-red-50 text-red-700 border-red-300">
                            <AlertTriangle className="h-3 w-3 mr-1" />
                            Invalid ({selectedShenanigan.successOdds + selectedShenanigan.failOdds + selectedShenanigan.backfireOdds}%)
                          </Badge>
                        )}
                      </div>
                    </div>

                    <Separator />

                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <Label>Min Range</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.minRange}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            minRange: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                      <div>
                        <Label>Max Range</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.maxRange}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            maxRange: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                    </div>

                    <div>
                      <Label>Weighted Range</Label>
                      <Input
                        type="text"
                        value={selectedShenanigan.weightedRange}
                        onChange={(e) => setSelectedShenanigan({
                          ...selectedShenanigan,
                          weightedRange: e.target.value
                        })}
                        placeholder="e.g., 3-5% or 1%:70%, 2%:25%, 3%:5%"
                      />
                    </div>

                    <Separator />

                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <Label>Min Target PP</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.minTarget}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            minTarget: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                      <div>
                        <Label>Per Round Limit</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.perRoundLimit}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            perRoundLimit: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                    </div>

                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <Label>Global Cooldown (sec)</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.cooldownGlobal}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            cooldownGlobal: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                      <div>
                        <Label>Per-Target Cooldown (sec)</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.cooldownPerTarget}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            cooldownPerTarget: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                    </div>

                    {selectedShenanigan.duration !== undefined && (
                      <div>
                        <Label>Duration (days)</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.duration}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            duration: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                    )}

                    {selectedShenanigan.protectionThreshold !== undefined && (
                      <div>
                        <Label>Protection Threshold (PP)</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.protectionThreshold}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            protectionThreshold: Math.max(0, parseInt(e.target.value) || 0)
                          })}
                          min="0"
                        />
                      </div>
                    )}

                    <div className="flex space-x-2">
                      <Button onClick={handleSaveShenanigan} className="flex-1">
                        <Save className="mr-2 h-4 w-4" />
                        Save Changes
                      </Button>
                    </div>
                  </CardContent>
                </Card>
              )}
            </div>
          </TabsContent>

          <TabsContent value="global" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle>Global Shenanigans Settings</CardTitle>
                <CardDescription>Configure system-wide parameters</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <Label>PP Budget Per Round</Label>
                    <Input
                      type="number"
                      value={globalSettings.ppBudgetPerRound}
                      onChange={(e) => setGlobalSettings({
                        ...globalSettings,
                        ppBudgetPerRound: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                    />
                    <p className="text-xs text-gray-600 mt-1">Total PP that can be minted per round</p>
                  </div>
                  <div>
                    <Label>Mirror Stack Cap</Label>
                    <Input
                      type="number"
                      value={globalSettings.mirrorStackCap}
                      onChange={(e) => setGlobalSettings({
                        ...globalSettings,
                        mirrorStackCap: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                    />
                    <p className="text-xs text-gray-600 mt-1">Maximum mirrors a player can own</p>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <Label>Loss Protection Threshold (PP)</Label>
                    <Input
                      type="number"
                      value={globalSettings.lossProtectionThreshold}
                      onChange={(e) => setGlobalSettings({
                        ...globalSettings,
                        lossProtectionThreshold: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                    />
                    <p className="text-xs text-gray-600 mt-1">Players below this PP are protected from loss effects</p>
                  </div>
                  <div>
                    <Label>Cooldown Protection (hours)</Label>
                    <Input
                      type="number"
                      value={globalSettings.cooldownProtectionHours}
                      onChange={(e) => setGlobalSettings({
                        ...globalSettings,
                        cooldownProtectionHours: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                    />
                    <p className="text-xs text-gray-600 mt-1">Hours of protection after negative effects</p>
                  </div>
                </div>

                <Separator />

                <div className="flex space-x-2">
                  <Button onClick={handleSaveGlobalSettings} className="flex-1">
                    <Save className="mr-2 h-4 w-4" />
                    Save Global Settings
                  </Button>
                  <Button onClick={handleResetToDefaults} variant="outline">
                    <RotateCcw className="mr-2 h-4 w-4" />
                    Reset All to Defaults
                  </Button>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="analytics" className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle>Shenanigans Analytics</CardTitle>
                <CardDescription>View usage statistics and event logs</CardDescription>
              </CardHeader>
              <CardContent>
                <div className="text-center py-8 text-gray-500">
                  <p className="text-lg font-semibold mb-2">Analytics Coming Soon</p>
                  <p className="text-sm">
                    This section will display filterable event logs, top shenanigans by casts/PP moved,
                    and CSV export functionality.
                  </p>
                </div>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}
