import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Separator } from '@/components/ui/separator';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { toast } from 'sonner';
import { Save, RotateCcw, AlertTriangle, CheckCircle, Info } from 'lucide-react';
import { useGetShenaniganConfigs, useUpdateShenaniganConfig, useSaveAllShenaniganConfigs, useResetShenaniganConfig } from '../hooks/useQueries';
import LoadingSpinner from './LoadingSpinner';

interface ShenaniganConfig {
  id: number;
  name: string;
  description: string;
  cost: number;
  successOdds: number;
  failureOdds: number;
  backfireOdds: number;
  duration: number;
  cooldown: number;
  effectValues: number[];
  castLimit: number;
  backgroundColor: string;
}

// Icon mapping for display
const shenaniganIcons: Record<number, string> = {
  0: '💰',
  1: '🌊',
  2: '✏️',
  3: '🏦',
  4: '🎯',
  5: '🪞',
  6: '⬆️',
  7: '✂️',
  8: '🐋',
  9: '📈',
  10: '✨',
};

export default function ShenanigansAdminPanel() {
  const { data: backendConfigs, isLoading } = useGetShenaniganConfigs();
  const updateConfig = useUpdateShenaniganConfig();
  const saveAllConfigs = useSaveAllShenaniganConfigs();
  const resetConfig = useResetShenaniganConfig();
  
  const [shenanigans, setShenanigans] = useState<ShenaniganConfig[]>([]);
  const [selectedShenanigan, setSelectedShenanigan] = useState<ShenaniganConfig | null>(null);

  // Load configs from backend
  useEffect(() => {
    if (backendConfigs) {
      const mappedConfigs = backendConfigs.map(config => ({
        id: Number(config.id),
        name: config.name,
        description: config.description,
        cost: config.cost,
        successOdds: Number(config.successOdds),
        failureOdds: Number(config.failureOdds),
        backfireOdds: Number(config.backfireOdds),
        duration: Number(config.duration),
        cooldown: Number(config.cooldown),
        effectValues: config.effectValues,
        castLimit: Number(config.castLimit),
        backgroundColor: config.backgroundColor,
      }));
      setShenanigans(mappedConfigs);
      
      // If a shenanigan is selected, update it with the latest data
      if (selectedShenanigan) {
        const updated = mappedConfigs.find(c => c.id === selectedShenanigan.id);
        if (updated) {
          setSelectedShenanigan(updated);
        }
      }
    }
  }, [backendConfigs]);

  const handleSaveShenanigan = async () => {
    if (!selectedShenanigan) return;

    // Validate odds sum to 100%
    const oddsSum = selectedShenanigan.successOdds + selectedShenanigan.failureOdds + selectedShenanigan.backfireOdds;
    if (oddsSum !== 100) {
      toast.error('Odds must sum to 100%');
      return;
    }

    // Validate all numeric values are non-negative
    if (selectedShenanigan.cost < 0 || selectedShenanigan.cooldown < 0 || selectedShenanigan.duration < 0 || selectedShenanigan.castLimit < 0) {
      toast.error('Numeric values cannot be negative');
      return;
    }

    try {
      // Save to backend
      await updateConfig.mutateAsync({
        id: BigInt(selectedShenanigan.id),
        name: selectedShenanigan.name,
        description: selectedShenanigan.description,
        cost: selectedShenanigan.cost,
        successOdds: BigInt(selectedShenanigan.successOdds),
        failureOdds: BigInt(selectedShenanigan.failureOdds),
        backfireOdds: BigInt(selectedShenanigan.backfireOdds),
        duration: BigInt(selectedShenanigan.duration),
        cooldown: BigInt(selectedShenanigan.cooldown),
        effectValues: selectedShenanigan.effectValues,
        castLimit: BigInt(selectedShenanigan.castLimit),
        backgroundColor: selectedShenanigan.backgroundColor,
      });

      // Update local state
      setShenanigans(prev => 
        prev.map(s => s.id === selectedShenanigan.id ? selectedShenanigan : s)
      );

      // Broadcast change event for real-time synchronization
      window.dispatchEvent(new CustomEvent('shenaniganUpdated', { 
        detail: {
          id: selectedShenanigan.id,
          name: selectedShenanigan.name,
          icon: shenaniganIcons[selectedShenanigan.id],
          description: selectedShenanigan.description,
          cost: selectedShenanigan.cost,
          successOdds: selectedShenanigan.successOdds,
          failOdds: selectedShenanigan.failureOdds,
          backfireOdds: selectedShenanigan.backfireOdds,
          effectValues: selectedShenanigan.effectValues.join(', '),
        }
      }));

      toast.success(`${selectedShenanigan.name} updated successfully`);
    } catch (error: any) {
      toast.error(`Failed to save: ${error.message || 'Unknown error'}`);
    }
  };

  const handleResetToDefaults = async () => {
    if (!selectedShenanigan) return;
    
    try {
      await resetConfig.mutateAsync(BigInt(selectedShenanigan.id));
      toast.success(`${selectedShenanigan.name} reset to defaults`);
    } catch (error: any) {
      toast.error(`Failed to reset: ${error.message || 'Unknown error'}`);
    }
  };

  const handleSaveAllChanges = async () => {
    // Validate all shenanigans
    for (const shen of shenanigans) {
      const oddsSum = shen.successOdds + shen.failureOdds + shen.backfireOdds;
      if (oddsSum !== 100) {
        toast.error(`${shen.name}: Odds must sum to 100%`);
        return;
      }
      if (shen.cost < 0 || shen.cooldown < 0 || shen.duration < 0 || shen.castLimit < 0) {
        toast.error(`${shen.name}: Numeric values cannot be negative`);
        return;
      }
    }

    try {
      // Save all to backend
      await saveAllConfigs.mutateAsync(shenanigans.map(shen => ({
        id: BigInt(shen.id),
        name: shen.name,
        description: shen.description,
        cost: shen.cost,
        successOdds: BigInt(shen.successOdds),
        failureOdds: BigInt(shen.failureOdds),
        backfireOdds: BigInt(shen.backfireOdds),
        duration: BigInt(shen.duration),
        cooldown: BigInt(shen.cooldown),
        effectValues: shen.effectValues,
        castLimit: BigInt(shen.castLimit),
        backgroundColor: shen.backgroundColor,
      })));

      // Broadcast all changes for real-time synchronization
      shenanigans.forEach(shen => {
        window.dispatchEvent(new CustomEvent('shenaniganUpdated', { 
          detail: {
            id: shen.id,
            name: shen.name,
            icon: shenaniganIcons[shen.id],
            description: shen.description,
            cost: shen.cost,
            successOdds: shen.successOdds,
            failOdds: shen.failureOdds,
            backfireOdds: shen.backfireOdds,
            effectValues: shen.effectValues.join(', '),
          }
        }));
      });

      toast.success('All changes saved successfully');
    } catch (error: any) {
      toast.error(`Failed to save all: ${error.message || 'Unknown error'}`);
    }
  };

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div className="text-center">
          <h2 className="text-2xl font-black text-white text-with-backdrop">
            🔧 Shenanigans Admin Panel 🔧
          </h2>
          <p className="text-center text-white text-with-backdrop mt-2">
            Configure all shenanigan parameters
          </p>
        </div>
        <div className="rewards-single-container">
          <div className="flex justify-center py-8">
            <LoadingSpinner />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="text-center">
        <h2 className="text-2xl font-black text-white text-with-backdrop">
          🔧 Shenanigans Admin Panel 🔧
        </h2>
        <p className="text-center text-white text-with-backdrop mt-2">
          Configure all shenanigan parameters
        </p>
      </div>

      <div className="rewards-single-container">
        <Alert className="mb-6 bg-yellow-50 border-2 border-yellow-400">
          <Info className="h-5 w-5 text-yellow-600" />
          <AlertDescription className="text-yellow-900">
            <div className="font-bold text-lg mb-2">⚠️ Admin Panel Instructions</div>
            <div className="space-y-2 text-sm">
              <p>
                Edit any shenanigan parameter below. All changes take effect immediately and are reflected on the main Shenanigans page in real-time.
              </p>
              <ul className="list-disc list-inside space-y-1 ml-4">
                <li>Success, Failure, and Backfire odds must sum to 100%</li>
                <li>All numeric values must be non-negative</li>
                <li>Use "Save Changes" to save individual shenanigans</li>
                <li>Use "Save All Changes" to save all modifications at once</li>
                <li>Use "Reset Defaults" to restore a shenanigan to its original values</li>
                <li>Changes are immediately visible on the main Shenanigans page</li>
                <li>Changes persist after navigating away and returning</li>
              </ul>
            </div>
          </AlertDescription>
        </Alert>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Shenanigans List */}
          <div className="lg:col-span-1">
            <Card>
              <CardHeader>
                <CardTitle>Shenanigans</CardTitle>
                <CardDescription>Select to edit</CardDescription>
              </CardHeader>
              <CardContent>
                <div className="space-y-2 max-h-[600px] overflow-y-auto">
                  {shenanigans.map((shen) => (
                    <div
                      key={shen.id}
                      onClick={() => setSelectedShenanigan(shen)}
                      className={`p-3 rounded-lg border-2 cursor-pointer transition-all ${
                        selectedShenanigan?.id === shen.id
                          ? 'border-yellow-500 bg-yellow-50'
                          : 'border-gray-200 hover:border-yellow-300'
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <div className="flex items-center space-x-3">
                          <span className="text-2xl">{shenaniganIcons[shen.id]}</span>
                          <div>
                            <div className="font-bold text-gray-900">{shen.name}</div>
                            <div className="text-xs text-gray-600">Cost: {shen.cost} PP</div>
                          </div>
                        </div>
                        <Badge variant="default">
                          Enabled
                        </Badge>
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Editor Panel */}
          {selectedShenanigan && (
            <div className="lg:col-span-2">
              <Card 
                style={{ 
                  backgroundColor: selectedShenanigan.backgroundColor,
                  border: '2px solid rgba(0, 0, 0, 0.1)'
                }}
              >
                <CardHeader>
                  <CardTitle className="flex items-center justify-between">
                    <span className="flex items-center" style={{ color: '#1a1a1a' }}>
                      <span className="text-2xl mr-2">{shenaniganIcons[selectedShenanigan.id]}</span>
                      {selectedShenanigan.name}
                    </span>
                    <Badge variant="default">
                      Enabled
                    </Badge>
                  </CardTitle>
                  <CardDescription style={{ color: '#4a4a4a' }}>
                    Edit all parameters for this shenanigan
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4 max-h-[600px] overflow-y-auto">
                  {/* Name */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Name</Label>
                    <Input
                      value={selectedShenanigan.name}
                      onChange={(e) => setSelectedShenanigan({
                        ...selectedShenanigan,
                        name: e.target.value
                      })}
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  {/* Description */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Description</Label>
                    <Textarea
                      value={selectedShenanigan.description}
                      onChange={(e) => setSelectedShenanigan({
                        ...selectedShenanigan,
                        description: e.target.value
                      })}
                      rows={3}
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  <Separator />

                  {/* Cost */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Cost (Ponzi Points)</Label>
                    <Input
                      type="number"
                      value={selectedShenanigan.cost}
                      onChange={(e) => setSelectedShenanigan({
                        ...selectedShenanigan,
                        cost: Math.max(0, parseFloat(e.target.value) || 0)
                      })}
                      min="0"
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  <Separator />

                  {/* Odds */}
                  <div>
                    <Label className="mb-2 block" style={{ color: '#1a1a1a', fontWeight: '600' }}>
                      Odds (must sum to 100%)
                    </Label>
                    <div className="grid grid-cols-3 gap-2">
                      <div>
                        <Label className="text-xs" style={{ color: '#4a4a4a' }}>Success %</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.successOdds}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            successOdds: Math.max(0, Math.min(100, parseInt(e.target.value) || 0))
                          })}
                          min="0"
                          max="100"
                          style={{ 
                            backgroundColor: 'rgba(255, 255, 255, 0.9)',
                            color: '#1a1a1a',
                            border: '1px solid rgba(0, 0, 0, 0.2)'
                          }}
                        />
                      </div>
                      <div>
                        <Label className="text-xs" style={{ color: '#4a4a4a' }}>Failure %</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.failureOdds}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            failureOdds: Math.max(0, Math.min(100, parseInt(e.target.value) || 0))
                          })}
                          min="0"
                          max="100"
                          style={{ 
                            backgroundColor: 'rgba(255, 255, 255, 0.9)',
                            color: '#1a1a1a',
                            border: '1px solid rgba(0, 0, 0, 0.2)'
                          }}
                        />
                      </div>
                      <div>
                        <Label className="text-xs" style={{ color: '#4a4a4a' }}>Backfire %</Label>
                        <Input
                          type="number"
                          value={selectedShenanigan.backfireOdds}
                          onChange={(e) => setSelectedShenanigan({
                            ...selectedShenanigan,
                            backfireOdds: Math.max(0, Math.min(100, parseInt(e.target.value) || 0))
                          })}
                          min="0"
                          max="100"
                          style={{ 
                            backgroundColor: 'rgba(255, 255, 255, 0.9)',
                            color: '#1a1a1a',
                            border: '1px solid rgba(0, 0, 0, 0.2)'
                          }}
                        />
                      </div>
                    </div>
                    <div className="mt-2">
                      {selectedShenanigan.successOdds + selectedShenanigan.failureOdds + selectedShenanigan.backfireOdds === 100 ? (
                        <Badge variant="outline" className="bg-green-50 text-green-700 border-green-300">
                          <CheckCircle className="h-3 w-3 mr-1" />
                          Valid (100%)
                        </Badge>
                      ) : (
                        <Badge variant="outline" className="bg-red-50 text-red-700 border-red-300">
                          <AlertTriangle className="h-3 w-3 mr-1" />
                          Invalid ({selectedShenanigan.successOdds + selectedShenanigan.failureOdds + selectedShenanigan.backfireOdds}%)
                        </Badge>
                      )}
                    </div>
                  </div>

                  <Separator />

                  {/* Duration */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Duration (hours, 0 for instant)</Label>
                    <Input
                      type="number"
                      value={selectedShenanigan.duration}
                      onChange={(e) => setSelectedShenanigan({
                        ...selectedShenanigan,
                        duration: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  {/* Cooldown Period */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Cooldown Period (hours)</Label>
                    <Input
                      type="number"
                      value={selectedShenanigan.cooldown}
                      onChange={(e) => setSelectedShenanigan({
                        ...selectedShenanigan,
                        cooldown: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  <Separator />

                  {/* Effect Values */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Effect Values (comma-separated numbers)</Label>
                    <Input
                      value={selectedShenanigan.effectValues.join(', ')}
                      onChange={(e) => {
                        const values = e.target.value.split(',').map(v => parseFloat(v.trim())).filter(v => !isNaN(v));
                        setSelectedShenanigan({
                          ...selectedShenanigan,
                          effectValues: values
                        });
                      }}
                      placeholder="e.g., 2.0, 8.0, 250.0"
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  {/* Cast Limit */}
                  <div>
                    <Label style={{ color: '#1a1a1a', fontWeight: '600' }}>Cast Limit (0 for unlimited)</Label>
                    <Input
                      type="number"
                      value={selectedShenanigan.castLimit}
                      onChange={(e) => setSelectedShenanigan({
                        ...selectedShenanigan,
                        castLimit: Math.max(0, parseInt(e.target.value) || 0)
                      })}
                      min="0"
                      style={{ 
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        color: '#1a1a1a',
                        border: '1px solid rgba(0, 0, 0, 0.2)'
                      }}
                    />
                  </div>

                  <Separator />

                  {/* Action Buttons */}
                  <div className="flex space-x-2">
                    <Button 
                      onClick={handleSaveShenanigan} 
                      className="flex-1 bg-green-600 hover:bg-green-700"
                      disabled={updateConfig.isPending}
                    >
                      <Save className="mr-2 h-4 w-4" />
                      {updateConfig.isPending ? 'Saving...' : 'Save Changes'}
                    </Button>
                    <Button 
                      onClick={handleResetToDefaults} 
                      variant="outline" 
                      className="flex-1"
                      disabled={resetConfig.isPending}
                    >
                      <RotateCcw className="mr-2 h-4 w-4" />
                      {resetConfig.isPending ? 'Resetting...' : 'Reset Defaults'}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </div>
          )}
        </div>

        {/* Save All Button */}
        <div className="mt-6">
          <Button 
            onClick={handleSaveAllChanges} 
            className="w-full bg-yellow-500 hover:bg-yellow-600 text-black font-bold text-lg py-6"
            disabled={saveAllConfigs.isPending}
          >
            <Save className="mr-2 h-5 w-5" />
            {saveAllConfigs.isPending ? 'Saving All...' : 'Save All Changes'}
          </Button>
        </div>
      </div>
    </div>
  );
}
