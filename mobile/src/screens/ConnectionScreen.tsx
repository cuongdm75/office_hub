import React, { useEffect, useState } from 'react';
import {
  View, Text, TextInput, TouchableOpacity,
  ActivityIndicator, Alert, StyleSheet, ScrollView,
} from 'react-native';
import * as SecureStore from 'expo-secure-store';
import { Platform } from 'react-native';

const Storage = {
  setItemAsync: async (key: string, value: string) => {
    if (Platform.OS === 'web') localStorage.setItem(key, value);
    else await SecureStore.setItemAsync(key, value);
  },
  getItemAsync: async (key: string) => {
    if (Platform.OS === 'web') return localStorage.getItem(key);
    return await SecureStore.getItemAsync(key);
  },
  deleteItemAsync: async (key: string) => {
    if (Platform.OS === 'web') localStorage.removeItem(key);
    else await SecureStore.deleteItemAsync(key);
  }
};
import { useSseStore } from '../store/sseStore';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Server, KeyRound, ArrowRight, Camera as CameraIcon, X, Wifi, RotateCcw } from 'lucide-react-native';
import { CameraView, useCameraPermissions } from 'expo-camera';

export function ConnectionScreen({ navigation }: any) {
  const [ipAddress, setIpAddress] = useState('');
  const [token, setToken] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [isScanning, setIsScanning] = useState(false);
  const [permission, requestPermission] = useCameraPermissions();

  const { connect, isConnecting, isConnected, error, clearError } = useSseStore();

  // ── Normalise URL → http://host:9002 (accepts ws://, bare IP, etc.) ───────
  const normaliseUrl = (raw: string): string => {
    let url = raw.trim();
    if (url.startsWith('ws://')) url = url.replace('ws://', 'http://');
    if (url.startsWith('wss://')) url = url.replace('wss://', 'https://');
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      url = `http://${url}`;
    }
    // *** FIX Bug #1: Migrate legacy port 9001 (WebSocket) → 9002 (SSE)
    url = url.replace(/:9001(\/|$)/, ':9002$1');
    // Add default port 9002 if no port specified
    const hostPart = url.replace(/https?:\/\//, '').split('/')[0];
    if (!/:\d+$/.test(hostPart)) url = url.replace(hostPart, `${hostPart}:9002`);
    return url;
  };

  // ── Reset / clear saved credentials ──────────────────────────────────────
  const handleReset = async () => {
    Alert.alert(
      'Reset Connection',
      'Xóa thông tin kết nối đã lưu và nhập lại?',
      [
        { text: 'Huỷ', style: 'cancel' },
        {
          text: 'Xóa', style: 'destructive',
          onPress: async () => {
            try {
              await Storage.deleteItemAsync('ws_urls');
              await Storage.deleteItemAsync('ws_token');
              await Storage.deleteItemAsync('ws_ip');
            } catch {}
            setIpAddress('');
            setToken('');
            clearError();
          },
        },
      ]
    );
  };

  // ── Auto-load saved credentials on mount ─────────────────────────────────
  useEffect(() => {
    (async () => {
      try {
        const savedUrlsStr = await Storage.getItemAsync('ws_urls');
        const savedToken   = await Storage.getItemAsync('ws_token');
        let urls: string[] = [];

        if (savedUrlsStr) {
          try {
            const parsed = JSON.parse(savedUrlsStr);
            if (Array.isArray(parsed) && parsed.length > 0) {
              urls = parsed.map(normaliseUrl);
              setIpAddress(urls[0]);
            }
          } catch {
            const n = normaliseUrl(savedUrlsStr);
            urls = [n]; setIpAddress(n);
          }
        } else {
          const saved = await Storage.getItemAsync('ws_ip');
          if (saved) { const n = normaliseUrl(saved); urls = [n]; setIpAddress(n); }
        }

        if (savedToken) setToken(savedToken);
        if (urls.length > 0 && savedToken) connect(urls, savedToken);
      } catch (e) { console.error('Load credentials failed', e); }
      finally { setIsLoading(false); }
    })();
  }, []);

  useEffect(() => { if (isConnected) navigation.replace('Home'); }, [isConnected]);
  useEffect(() => {
    if (error) Alert.alert('Connection Error', error, [{ text: 'OK', onPress: clearError }]);
  }, [error]);

  // ── Manual connect ───────────────────────────────────────────────────────
  const handleConnect = async (rawUrls: string | string[] = [ipAddress], tok = token) => {
    const urls = (Array.isArray(rawUrls) ? rawUrls : [rawUrls]).map(normaliseUrl).filter(Boolean);
    if (!urls[0] || !tok) {
      Alert.alert('Validation Error', 'Please enter Server Address and Token');
      return;
    }
    try {
      await Storage.setItemAsync('ws_urls', JSON.stringify(urls));
      await Storage.setItemAsync('ws_token', tok);
    } catch {}
    connect(urls, tok);
  };

  // ── QR scanner ───────────────────────────────────────────────────────────
  const handleBarcodeScanned = ({ data }: { type: string; data: string }) => {
    setIsScanning(false);
    try {
      const payload = JSON.parse(data);
      if (payload.type === 'office-hub-pairing' && payload.urls?.length > 0) {
        const normUrls: string[] = payload.urls.map(normaliseUrl);
        // Prefer LAN over Tailscale (100.x)
        const sorted = [...normUrls].sort((a, b) => {
          const ta = a.includes('//100.'); const tb = b.includes('//100.');
          return ta && !tb ? 1 : !ta && tb ? -1 : 0;
        });
        setIpAddress(sorted[0]);
        if (payload.token) setToken(payload.token);
        Alert.alert('QR Scanned ✓', `${sorted.length} address(es) found.\nConnecting to: ${sorted[0]}`);
        handleConnect(sorted, payload.token || token);
      } else {
        Alert.alert('Invalid QR Code', 'Not a valid Office Hub pairing code.');
      }
    } catch {
      Alert.alert('Invalid QR Code', 'Could not parse QR data.');
    }
  };

  const openScanner = async () => {
    if (!permission?.granted) {
      const { granted } = await requestPermission();
      if (!granted) { Alert.alert('Permission Denied', 'Camera access is required.'); return; }
    }
    setIsScanning(true);
  };

  // ── Loading ───────────────────────────────────────────────────────────────
  if (isLoading) return (
    <View style={s.loading}>
      <ActivityIndicator size="large" color="#3b82f6" />
      <Text style={s.loadingText}>Loading saved connection…</Text>
    </View>
  );

  // ── Scanner ───────────────────────────────────────────────────────────────
  if (isScanning) return (
    <SafeAreaView style={s.scannerRoot}>
      <View style={s.flex}>
        <CameraView
          style={StyleSheet.absoluteFillObject}
          facing="back"
          barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
          onBarcodeScanned={handleBarcodeScanned}
        />
        <View style={s.scanOverlay}><View style={s.scanFrame} /></View>
        <TouchableOpacity style={s.closeBtn} onPress={() => setIsScanning(false)}>
          <X size={24} color="white" />
        </TouchableOpacity>
        <View style={s.scanHintWrap}>
          <Text style={s.scanHintText}>Align the Office Hub QR Code</Text>
        </View>
      </View>
    </SafeAreaView>
  );

  // ── Main form ─────────────────────────────────────────────────────────────
  return (
    <SafeAreaView style={s.root}>
      <ScrollView contentContainerStyle={s.container} keyboardShouldPersistTaps="handled">

        {/* Hero */}
        <View style={s.hero}>
          <View style={s.logoRing}><Wifi size={32} color="#3b82f6" /></View>
          <Text style={s.title}>Office Hub</Text>
          <Text style={s.subtitle}>Connect to your desktop AI orchestrator</Text>
        </View>

        {/* Card */}
        <View style={s.card}>
          <TouchableOpacity onPress={openScanner} style={s.qrBtn}>
            <CameraIcon size={20} color="white" />
            <Text style={s.qrBtnText}>Scan Pairing QR Code</Text>
          </TouchableOpacity>

          <View style={s.divider}>
            <View style={s.divLine} /><Text style={s.divText}>OR ENTER MANUALLY</Text><View style={s.divLine} />
          </View>

          {/* Address */}
          <View style={s.field}>
            <Text style={s.label}>Server Address</Text>
            <View style={s.row}>
              <Server size={18} color="#64748b" />
              <TextInput
                style={s.input}
                placeholder="http://192.168.1.X:9002"
                placeholderTextColor="#475569"
                value={ipAddress}
                onChangeText={setIpAddress}
                keyboardType="url"
                autoCapitalize="none"
                autoCorrect={false}
              />
            </View>
            <Text style={s.hint}>Port 9002  ·  REST + SSE transport</Text>
          </View>

          {/* Token */}
          <View style={s.field}>
            <Text style={s.label}>Access Token</Text>
            <View style={s.row}>
              <KeyRound size={18} color="#64748b" />
              <TextInput
                style={s.input}
                placeholder="Secret Token"
                placeholderTextColor="#475569"
                value={token}
                onChangeText={setToken}
                secureTextEntry
                autoCapitalize="none"
              />
            </View>
          </View>

          {/* Submit */}
          <TouchableOpacity
            onPress={() => handleConnect()}
            disabled={isConnecting}
            style={[s.submitBtn, isConnecting && s.submitBusy]}
          >
            {isConnecting ? (
              <><ActivityIndicator size="small" color="white" /><Text style={s.submitText}>Connecting…</Text></>
            ) : (
              <><Text style={s.submitText}>Connect</Text><ArrowRight size={20} color="white" /></>
            )}
          </TouchableOpacity>
        </View>

        <TouchableOpacity onPress={handleReset} style={s.resetBtn}>
          <RotateCcw size={14} color="#475569" />
          <Text style={s.resetText}>Reset Connection</Text>
        </TouchableOpacity>
        <Text style={s.footer}>Office Hub  ·  MCP-Hybrid SSE+REST</Text>
      </ScrollView>
    </SafeAreaView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#0f172a' },
  flex: { flex: 1 },
  loading: { flex: 1, justifyContent: 'center', alignItems: 'center', backgroundColor: '#0f172a', gap: 16 },
  loadingText: { color: '#64748b', fontSize: 14 },
  scannerRoot: { flex: 1, backgroundColor: 'black' },
  container: { flexGrow: 1, padding: 24, justifyContent: 'center' },

  // Hero
  hero: { alignItems: 'center', marginBottom: 40 },
  logoRing: {
    width: 72, height: 72, borderRadius: 36,
    backgroundColor: 'rgba(59,130,246,0.1)',
    borderWidth: 1, borderColor: 'rgba(59,130,246,0.2)',
    alignItems: 'center', justifyContent: 'center', marginBottom: 20,
  },
  title: { fontSize: 32, fontWeight: '800', color: 'white', letterSpacing: -1 },
  subtitle: { color: '#64748b', textAlign: 'center', marginTop: 8, fontSize: 14 },

  // Card
  card: {
    backgroundColor: '#1e293b', padding: 24, borderRadius: 20,
    borderWidth: 1, borderColor: '#334155',
    shadowColor: '#000', shadowOffset: { width: 0, height: 8 },
    shadowOpacity: 0.3, shadowRadius: 16,
  },

  // QR
  qrBtn: {
    flexDirection: 'row', justifyContent: 'center', alignItems: 'center',
    paddingVertical: 14, backgroundColor: '#334155', borderRadius: 12,
    borderWidth: 1, borderColor: '#475569', marginBottom: 24, gap: 8,
  },
  qrBtnText: { color: 'white', fontWeight: '600', fontSize: 15 },

  // Divider
  divider: { flexDirection: 'row', alignItems: 'center', marginBottom: 20 },
  divLine: { flex: 1, height: 1, backgroundColor: '#334155' },
  divText: { color: '#475569', marginHorizontal: 12, fontWeight: '600', fontSize: 11, letterSpacing: 1 },

  // Field
  field: { marginBottom: 16 },
  label: { color: '#94a3b8', fontWeight: '600', marginBottom: 8, fontSize: 13 },
  hint: { color: '#475569', fontSize: 11, marginTop: 6 },
  row: {
    flexDirection: 'row', alignItems: 'center', backgroundColor: '#0f172a',
    borderRadius: 12, paddingHorizontal: 14, borderWidth: 1, borderColor: '#334155', gap: 10,
  },
  input: { flex: 1, paddingVertical: 14, color: 'white', fontSize: 14 },

  // Submit
  submitBtn: {
    flexDirection: 'row', justifyContent: 'center', alignItems: 'center',
    paddingVertical: 16, borderRadius: 12, backgroundColor: '#2563eb', marginTop: 8, gap: 8,
  },
  submitBusy: { backgroundColor: '#1d4ed8' },
  submitText: { color: 'white', fontWeight: '700', fontSize: 16 },

  // Scanner
  scanOverlay: { ...StyleSheet.absoluteFillObject, justifyContent: 'center', alignItems: 'center' },
  scanFrame: { width: 260, height: 260, borderRadius: 16, borderWidth: 3, borderColor: '#3b82f6' },
  closeBtn: {
    position: 'absolute', top: 40, right: 24,
    backgroundColor: 'rgba(30,41,59,0.85)', padding: 12, borderRadius: 24,
  },
  scanHintWrap: { position: 'absolute', bottom: 80, left: 0, right: 0, alignItems: 'center' },
  scanHintText: {
    color: 'white', fontWeight: '500', fontSize: 15,
    backgroundColor: 'rgba(15,23,42,0.85)', paddingHorizontal: 24, paddingVertical: 12, borderRadius: 24,
  },

  resetBtn: {
    flexDirection: 'row', alignItems: 'center', justifyContent: 'center',
    gap: 6, marginTop: 24, paddingVertical: 8,
  },
  resetText: { color: '#475569', fontSize: 12 },
  footer: { textAlign: 'center', color: '#334155', fontSize: 11, marginTop: 12 },
});
