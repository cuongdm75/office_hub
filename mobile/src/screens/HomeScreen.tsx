import React from 'react';
import { View, Text, TouchableOpacity, Alert, ScrollView, StyleSheet } from 'react-native';
import { useSseStore } from '../store/sseStore';
import { SafeAreaView } from 'react-native-safe-area-context';
import { PowerOff, CheckCircle2, AlertCircle } from 'lucide-react-native';

export function ApprovalsScreen({ navigation }: any) {
  const { disconnect, isConnected, activeHitlRequest } = useSseStore();

  const handleDisconnect = () => {
    Alert.alert('Disconnect', 'Are you sure you want to disconnect?', [
      { text: 'Cancel', style: 'cancel' },
      { text: 'Disconnect', style: 'destructive', onPress: () => { disconnect(); navigation.replace('Connection'); } },
    ]);
  };

  return (
    <SafeAreaView style={s.root} edges={['top']}>
      <View style={s.header}>
        <View style={s.statusRow}>
          <View style={[s.statusDot, isConnected ? s.dotGreen : s.dotRed]} />
          <Text style={s.statusText}>{isConnected ? 'Connected' : 'Disconnected'}</Text>
        </View>
        <TouchableOpacity onPress={handleDisconnect} style={s.disconnectBtn}>
          <PowerOff size={20} color="#ef4444" />
        </TouchableOpacity>
      </View>

      <ScrollView style={s.flex} contentContainerStyle={s.content}>
        {activeHitlRequest ? (
          <View style={s.approvalCard}>
            <View style={s.approvalTitleRow}>
              <AlertCircle color="#f59e0b" size={20} />
              <Text style={s.approvalTitle}>Pending Approval</Text>
            </View>
            <Text style={s.riskBadge}>{activeHitlRequest.risk_level} Risk</Text>
            <Text style={s.approvalDesc}>{activeHitlRequest.description}</Text>
            <Text style={s.approvalHint}>Review using the popup sheet...</Text>
          </View>
        ) : (
          <View style={s.emptyCard}>
            <CheckCircle2 size={48} color="#10b981" />
            <Text style={s.emptyTitle}>All Caught Up</Text>
            <Text style={s.emptyDesc}>There are no pending approvals requiring your attention at this time.</Text>
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#0f172a' },
  flex: { flex: 1 },
  header: {
    flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center',
    padding: 16, backgroundColor: '#1e293b', borderBottomWidth: 1, borderBottomColor: '#334155',
  },
  statusRow: { flexDirection: 'row', alignItems: 'center' },
  statusDot: { width: 12, height: 12, borderRadius: 6, marginRight: 8 },
  dotGreen: { backgroundColor: '#22c55e' },
  dotRed: { backgroundColor: '#ef4444' },
  statusText: { color: 'white', fontWeight: '600', fontSize: 18 },
  disconnectBtn: { padding: 8, backgroundColor: '#1e293b', borderRadius: 20 },
  content: { padding: 16 },
  approvalCard: {
    backgroundColor: '#1e293b', borderWidth: 1, borderColor: 'rgba(245,158,11,0.5)',
    borderRadius: 16, padding: 16, marginBottom: 16,
  },
  approvalTitleRow: { flexDirection: 'row', alignItems: 'center', marginBottom: 8, gap: 8 },
  approvalTitle: { color: 'white', fontWeight: 'bold', fontSize: 18 },
  riskBadge: { color: '#fbbf24', fontWeight: '500', fontSize: 11, textTransform: 'uppercase', letterSpacing: 1, marginBottom: 8 },
  approvalDesc: { color: '#cbd5e1' },
  approvalHint: { color: '#64748b', fontSize: 12, marginTop: 16 },
  emptyCard: {
    backgroundColor: 'rgba(30,41,59,0.5)', borderRadius: 16, padding: 32,
    alignItems: 'center', borderWidth: 1, borderColor: '#334155', marginTop: 40,
  },
  emptyTitle: { color: 'white', fontSize: 20, fontWeight: 'bold', marginTop: 16, marginBottom: 8 },
  emptyDesc: { color: '#94a3b8', textAlign: 'center' },
});
