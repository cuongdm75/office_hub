import React from 'react';
import { View, Text, TouchableOpacity, ScrollView, StyleSheet, Alert } from 'react-native';
import { Settings, Server, Shield, Wifi, LogOut, ChevronRight, Radio } from 'lucide-react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { useSseStore } from '../store/sseStore';

interface RowProps { icon: any; title: string; value?: string; onPress?: () => void; danger?: boolean; }
function SettingRow({ icon: Icon, title, value, onPress, danger }: RowProps) {
  return (
    <TouchableOpacity
      style={s.row}
      onPress={onPress}
      disabled={!onPress}
      activeOpacity={onPress ? 0.7 : 1}
    >
      <View style={[s.rowIcon, danger && s.rowIconDanger]}>
        <Icon color={danger ? '#ef4444' : '#94a3b8'} size={16} />
      </View>
      <View style={s.rowContent}>
        <Text style={[s.rowTitle, danger && s.rowTitleDanger]}>{title}</Text>
        {!!value && <Text style={s.rowValue} numberOfLines={1}>{value}</Text>}
      </View>
      {onPress && <ChevronRight color="#475569" size={18} />}
    </TouchableOpacity>
  );
}

export function SettingsScreen({ navigation }: any) {
  const { disconnect, currentBaseUrl, isConnected } = useSseStore();

  const handleDisconnect = () => {
    Alert.alert('Disconnect', 'Close the current session and return to the connection screen?', [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Disconnect', style: 'destructive',
        onPress: () => { disconnect(); navigation.reset({ index: 0, routes: [{ name: 'Connection' }] }); },
      },
    ]);
  };

  const connectionStatus = isConnected
    ? `Connected  ·  SSE+REST`
    : 'Not connected';

  return (
    <SafeAreaView style={s.root} edges={['top']}>
      {/* Header */}
      <View style={s.header}>
        <View>
          <Text style={s.headerTitle}>Settings</Text>
          <Text style={s.headerSub}>PREFERENCES & CONNECTION</Text>
        </View>
        <Settings color="#475569" size={22} />
      </View>

      <ScrollView style={{ flex: 1 }} contentContainerStyle={{ paddingBottom: 40 }}>
        {/* Connection status banner */}
        <View style={[s.banner, isConnected ? s.bannerConnected : s.bannerDisconnected]}>
          <View style={[s.dot, isConnected ? s.dotGreen : s.dotRed]} />
          <Text style={s.bannerText}>{connectionStatus}</Text>
        </View>

        {/* Connection section */}
        <Text style={s.sectionLabel}>Connection</Text>
        <View style={s.section}>
          <SettingRow
            icon={Server}
            title="Server Address"
            value={currentBaseUrl || 'Not set'}
          />
          <SettingRow
            icon={Radio}
            title="Transport Protocol"
            value="SSE (downlink)  +  REST (uplink)"
          />
          <SettingRow
            icon={Shield}
            title="Authentication"
            value="Bearer token  ·  Configured"
          />
          <SettingRow
            icon={Wifi}
            title="Network Mode"
            value="LAN / Tailscale auto-detect"
          />
        </View>

        {/* LLM Routing section */}
        <Text style={s.sectionLabel}>AI Engine & Routing</Text>
        <View style={s.section}>
          <SettingRow
            icon={Server}
            title="Model Routing Tier"
            value="Fast, Default & Reasoning"
          />
          <SettingRow
            icon={Radio}
            title="Dashboard & Metrics"
            value="View on Desktop app"
          />
        </View>

        {/* Disconnect */}
        <View style={{ paddingHorizontal: 16, marginTop: 32 }}>
          <TouchableOpacity onPress={handleDisconnect} style={s.disconnectBtn}>
            <LogOut color="#ef4444" size={18} />
            <Text style={s.disconnectText}>Disconnect from Server</Text>
          </TouchableOpacity>
        </View>

        <Text style={s.version}>Office Hub  ·  v1.0.0  ·  MCP-Hybrid</Text>
      </ScrollView>
    </SafeAreaView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#0f172a' },
  header: {
    paddingHorizontal: 24, paddingVertical: 16,
    borderBottomWidth: 1, borderBottomColor: '#1e293b',
    backgroundColor: '#0f172a', flexDirection: 'row',
    alignItems: 'center', justifyContent: 'space-between',
  },
  headerTitle: { color: 'white', fontSize: 18, fontWeight: '600', letterSpacing: -0.3 },
  headerSub: { color: '#475569', fontSize: 11, fontWeight: '600', textTransform: 'uppercase', letterSpacing: 1, marginTop: 2 },

  banner: {
    flexDirection: 'row', alignItems: 'center', gap: 10,
    marginHorizontal: 16, marginTop: 16, marginBottom: 4,
    padding: 14, borderRadius: 12, borderWidth: 1,
  },
  bannerConnected: { backgroundColor: 'rgba(16,185,129,0.08)', borderColor: 'rgba(16,185,129,0.2)' },
  bannerDisconnected: { backgroundColor: 'rgba(239,68,68,0.08)', borderColor: 'rgba(239,68,68,0.2)' },
  dot: { width: 8, height: 8, borderRadius: 4 },
  dotGreen: { backgroundColor: '#10b981' },
  dotRed: { backgroundColor: '#ef4444' },
  bannerText: { color: '#94a3b8', fontSize: 13 },

  sectionLabel: {
    color: '#475569', fontSize: 11, fontWeight: '600',
    textTransform: 'uppercase', letterSpacing: 1,
    marginTop: 20, marginBottom: 8, paddingHorizontal: 20,
  },
  section: {
    marginHorizontal: 16, backgroundColor: '#1e293b',
    borderRadius: 12, borderWidth: 1, borderColor: '#334155', overflow: 'hidden',
  },
  row: {
    flexDirection: 'row', alignItems: 'center',
    paddingVertical: 14, paddingHorizontal: 16,
    borderBottomWidth: 1, borderBottomColor: '#334155',
  },
  rowIcon: {
    width: 32, height: 32, borderRadius: 8, backgroundColor: '#334155',
    alignItems: 'center', justifyContent: 'center', marginRight: 12,
  },
  rowIconDanger: { backgroundColor: 'rgba(239,68,68,0.15)' },
  rowContent: { flex: 1 },
  rowTitle: { color: 'white', fontSize: 14, fontWeight: '500' },
  rowTitleDanger: { color: '#ef4444' },
  rowValue: { color: '#64748b', fontSize: 12, marginTop: 2 },

  disconnectBtn: {
    flexDirection: 'row', alignItems: 'center', justifyContent: 'center',
    backgroundColor: 'rgba(239,68,68,0.08)', borderWidth: 1,
    borderColor: 'rgba(239,68,68,0.2)', padding: 16, borderRadius: 12, gap: 8,
  },
  disconnectText: { color: '#ef4444', fontWeight: '600', fontSize: 15 },
  version: { textAlign: 'center', color: '#334155', fontSize: 11, marginTop: 24 },
});
