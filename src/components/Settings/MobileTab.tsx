import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RefreshCw, Network, QrCode, Copy, Check, Smartphone } from 'lucide-react';
import toast from 'react-hot-toast';
import { QRCodeSVG } from 'qrcode.react';

interface NetworkInfo {
  lanIps: string[];
  tailscaleIp: string | null;
  tailscaleHostname: string | null;
  wsPort: number;
  preferredAddress: string;
}

interface TailscaleState {
  installed: boolean;
  running: boolean;
  connected: boolean;
  ipV4: string | null;
  dnsHostname: string | null;
  error: string | null;
}

interface SystemStatus {
  network: NetworkInfo;
  tailscale: TailscaleState;
}

interface QrPayload {
  qrData: string;
  qrSvg: string;
  pairingInfo: {
    url: string;       // primary SSE URL — matches Rust `pub url` → camelCase `url`
    urls: string[];    // all candidate URLs
    token: string | null;
    expiresAt: string; // ISO 8601 from chrono
    version: string;
  };
}

export default function MobileTab() {
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [qrPayload, setQrPayload] = useState<QrPayload | null>(null);
  const [copied, setCopied] = useState(false);
  const [copiedUrl, setCopiedUrl] = useState(false);

  useEffect(() => {
    loadStatus();
  }, []);

  const loadStatus = async () => {
    try {
      setIsLoading(true);
      const data: SystemStatus = await invoke('get_system_status');
      setStatus(data);
    } catch (error) {
      console.error('Failed to load system status:', error);
      toast.error('Failed to load system status');
    } finally {
      setIsLoading(false);
    }
  };

  const handleGenerateQr = async () => {
    try {
      const data: QrPayload = await invoke('get_pairing_qr');
      setQrPayload(data);
    } catch (error) {
      console.error('Failed to generate QR code:', error);
      toast.error('Failed to generate QR code');
    }
  };

  if (isLoading || !status) {
    return (
      <div className="flex items-center justify-center h-full min-h-[400px]">
        <RefreshCw className="animate-spin text-slate-400" size={24} />
      </div>
    );
  }

  return (
    <div className="space-y-8 animate-in fade-in duration-300">
      <div className="flex justify-between items-center mb-6">
        <div>
          <h2 className="text-xl font-bold text-slate-800 dark:text-slate-100">Mobile & Remote Access</h2>
          <p className="text-slate-500 dark:text-slate-400 mt-1">Connect your mobile device to Office Hub.</p>
        </div>
        <button
          onClick={loadStatus}
          className="flex items-center px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 rounded-lg font-medium transition-colors"
        >
          <RefreshCw className="mr-2" size={18} />
          Refresh Network
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
        
        {/* Left Column: Network Info */}
        <div className="space-y-8">
          <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
            <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
              <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
                <Network className="mr-2 text-slate-500" size={20} />
                Network Interfaces
              </h2>
            </div>
            <div className="p-6 space-y-4">
              <div>
                <p className="text-sm font-medium text-slate-500 dark:text-slate-400">WebSocket Port</p>
                <p className="text-lg text-slate-800 dark:text-slate-200 font-mono mt-1">{status.network.wsPort}</p>
              </div>
              
              <div>
                <p className="text-sm font-medium text-slate-500 dark:text-slate-400">Local Area Network (LAN)</p>
                <div className="mt-2 space-y-2">
                  {status.network.lanIps.map(ip => (
                    <div key={ip} className="px-3 py-2 bg-slate-50 dark:bg-slate-900 rounded border border-slate-200 dark:border-slate-700 font-mono text-sm">
                      {ip}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>

          <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden">
            <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50 flex justify-between items-center">
              <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
                <span className="w-5 h-5 mr-2 inline-flex justify-center items-center rounded-full bg-slate-800 text-white text-xs">T</span>
                Tailscale Status
              </h2>
              <div className={`px-2 py-1 text-xs font-medium rounded-full ${status.tailscale.connected ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/30 dark:text-emerald-400' : 'bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-400'}`}>
                {status.tailscale.connected ? 'Connected' : 'Offline'}
              </div>
            </div>
            <div className="p-6 space-y-4">
              {!status.tailscale.installed ? (
                <div className="text-sm text-slate-500 dark:text-slate-400">
                  <p>Tailscale is not installed. Install Tailscale to securely connect to Office Hub from anywhere.</p>
                  <a href="https://tailscale.com/download" target="_blank" rel="noreferrer" className="text-blue-600 hover:underline mt-2 inline-block">Download Tailscale</a>
                </div>
              ) : (
                <>
                  {status.tailscale.dnsHostname && (
                    <div>
                      <p className="text-sm font-medium text-slate-500 dark:text-slate-400">Hostname</p>
                      <p className="text-lg text-slate-800 dark:text-slate-200 font-mono mt-1">{status.tailscale.dnsHostname}</p>
                    </div>
                  )}
                  {status.tailscale.ipV4 && (
                    <div>
                      <p className="text-sm font-medium text-slate-500 dark:text-slate-400">Tailscale IP</p>
                      <p className="text-lg text-slate-800 dark:text-slate-200 font-mono mt-1">{status.tailscale.ipV4}</p>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </div>

        {/* Right Column: QR Code */}
        <div className="bg-white dark:bg-slate-950 rounded-2xl border border-slate-200 dark:border-slate-800 shadow-sm overflow-hidden flex flex-col">
          <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-900/50">
            <h2 className="text-lg font-semibold flex items-center text-slate-800 dark:text-slate-200">
              <QrCode className="mr-2 text-slate-500" size={20} />
              Pair Device
            </h2>
          </div>
          
          <div className="p-8 flex-1 flex flex-col items-center justify-center space-y-6">
            {!qrPayload ? (
              <div className="text-center space-y-4">
                <div className="w-20 h-20 bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 rounded-full flex items-center justify-center mx-auto mb-4">
                  <Smartphone size={40} />
                </div>
                <h3 className="text-lg font-medium">Ready to Pair</h3>
                <p className="text-slate-500 dark:text-slate-400 max-w-sm mx-auto">
                  Generate a temporary QR code to securely pair your mobile device with Office Hub.
                </p>
                <button
                  onClick={handleGenerateQr}
                  className="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition-colors mt-4"
                >
                  Generate QR Code
                </button>
              </div>
            ) : (
              <div className="text-center space-y-6 flex flex-col items-center w-full">
                <div className="bg-white p-4 rounded-xl shadow-sm border border-slate-200 inline-block">
                  <QRCodeSVG 
                    value={qrPayload.qrData} 
                    size={240} 
                    level="Q"
                    includeMargin={false}
                  />
                </div>
                
                {/* URL display */}
                <div className="w-full text-left space-y-1">
                  <p className="text-xs font-semibold text-[var(--text-muted)] uppercase tracking-wide">Server Address</p>
                  <div className="flex items-center gap-2">
                    <code className="flex-1 text-xs bg-[var(--bg-input)] px-3 py-2 rounded-lg text-[var(--text-primary)] break-all border border-[var(--border-default)]">
                      {qrPayload.pairingInfo.url}
                    </code>
                    <button
                      onClick={() => {
                        navigator.clipboard?.writeText(qrPayload.pairingInfo.url);
                        setCopiedUrl(true); setTimeout(() => setCopiedUrl(false), 2000);
                      }}
                      className="p-2 rounded-lg bg-[var(--bg-input)] hover:bg-[var(--bg-hover)] border border-[var(--border-default)] transition-colors shrink-0"
                      title="Copy URL"
                    >
                      {copiedUrl ? <Check size={14} className="text-emerald-500" /> : <Copy size={14} className="text-[var(--text-muted)]" />}
                    </button>
                  </div>
                </div>

                {/* Token display — critical for manual entry */}
                {qrPayload.pairingInfo.token && (
                  <div className="w-full text-left space-y-1">
                    <p className="text-xs font-semibold text-[var(--text-muted)] uppercase tracking-wide">Access Token</p>
                    <div className="flex items-center gap-2">
                      <code className="flex-1 text-xs bg-[var(--bg-input)] px-3 py-2 rounded-lg text-[var(--text-primary)] break-all border border-[var(--border-default)] font-mono">
                        {qrPayload.pairingInfo.token}
                      </code>
                      <button
                        onClick={() => {
                          navigator.clipboard?.writeText(qrPayload.pairingInfo.token!);
                          setCopied(true); setTimeout(() => setCopied(false), 2000);
                          toast.success('Token copied!');
                        }}
                        className="p-2 rounded-lg bg-[var(--bg-input)] hover:bg-[var(--bg-hover)] border border-[var(--border-default)] transition-colors shrink-0"
                        title="Copy token"
                      >
                        {copied ? <Check size={14} className="text-emerald-500" /> : <Copy size={14} className="text-[var(--text-muted)]" />}
                      </button>
                    </div>
                    <p className="text-xs text-[var(--text-muted)]">Paste this into the mobile app if QR auto-connect fails.</p>
                  </div>
                )}

                <p className="text-xs text-orange-600 dark:text-orange-400">
                  Expires at {new Date(qrPayload.pairingInfo.expiresAt).toLocaleTimeString()}
                </p>

                <button
                  onClick={handleGenerateQr}
                  className="px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 rounded-lg text-sm font-medium transition-colors"
                >
                  Regenerate
                </button>
              </div>
            )}
          </div>
        </div>

      </div>
    </div>
  );
}
