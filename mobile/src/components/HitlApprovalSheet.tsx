import React, { useState } from 'react';
import {
  View, Text, TouchableOpacity, TextInput,
  Modal, KeyboardAvoidingView, Platform, ScrollView, StyleSheet,
} from 'react-native';
import { useSseStore } from '../store/sseStore';
import { AlertTriangle, Check, X } from 'lucide-react-native';

export function HitlApprovalSheet() {
  const { activeHitlRequest, sendHitlResponse } = useSseStore();
  const [input, setInput] = useState('');

  if (!activeHitlRequest) return null;

  const handleApprove = () => { sendHitlResponse(activeHitlRequest.action_id, true, input); setInput(''); };
  const handleDeny = () => { sendHitlResponse(activeHitlRequest.action_id, false, input); setInput(''); };

  return (
    <Modal visible={!!activeHitlRequest} transparent animationType="slide">
      <KeyboardAvoidingView
        behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
        style={s.backdrop}
      >
        <View style={s.sheet}>
          <View style={s.sheetHeader}>
            <View style={s.iconWrap}><AlertTriangle size={28} color="#f59e0b" /></View>
            <View style={s.flex}>
              <Text style={s.sheetTitle}>Approval Required</Text>
              <Text style={s.riskText}>Risk: {activeHitlRequest.risk_level.toUpperCase()}</Text>
            </View>
          </View>

          <ScrollView style={s.scrollArea}>
            <View style={s.detailsCard}>
              <Text style={s.detailsLabel}>Details</Text>
              <Text style={s.detailsText}>{activeHitlRequest.description}</Text>
            </View>
            <View style={s.inputCard}>
              <TextInput
                style={s.textInput}
                placeholder="Add optional comments or instructions..."
                placeholderTextColor="#64748b"
                multiline
                value={input}
                onChangeText={setInput}
                textAlignVertical="top"
              />
            </View>
          </ScrollView>

          <View style={s.actionRow}>
            <TouchableOpacity onPress={handleDeny} style={s.denyBtn}>
              <X size={24} color="#ef4444" />
              <Text style={s.denyText}>Deny</Text>
            </TouchableOpacity>
            <TouchableOpacity onPress={handleApprove} style={s.approveBtn}>
              <Check size={24} color="white" />
              <Text style={s.approveText}>Approve</Text>
            </TouchableOpacity>
          </View>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

const s = StyleSheet.create({
  backdrop: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(0,0,0,0.6)' },
  sheet: {
    backgroundColor: '#0f172a', borderTopLeftRadius: 24, borderTopRightRadius: 24,
    padding: 24, borderTopWidth: 1, borderTopColor: '#334155', maxHeight: '90%',
  },
  flex: { flex: 1 },
  sheetHeader: { flexDirection: 'row', alignItems: 'center', marginBottom: 24 },
  iconWrap: { backgroundColor: 'rgba(245,158,11,0.2)', padding: 12, borderRadius: 24, marginRight: 16 },
  sheetTitle: { color: 'white', fontWeight: 'bold', fontSize: 20 },
  riskText: { color: '#94a3b8', fontWeight: '500' },
  scrollArea: { marginBottom: 24 },
  detailsCard: { backgroundColor: '#1e293b', padding: 16, borderRadius: 12, borderWidth: 1, borderColor: '#334155', marginBottom: 16 },
  detailsLabel: { color: '#94a3b8', fontSize: 11, textTransform: 'uppercase', letterSpacing: 1, fontWeight: 'bold', marginBottom: 8 },
  detailsText: { color: '#e2e8f0', fontSize: 15 },
  inputCard: { backgroundColor: '#1e293b', borderRadius: 12, borderWidth: 1, borderColor: '#334155', padding: 8 },
  textInput: { color: 'white', padding: 12, minHeight: 80 },
  actionRow: { flexDirection: 'row', gap: 16, paddingBottom: 24 },
  denyBtn: {
    flex: 1, backgroundColor: 'rgba(239,68,68,0.1)', borderWidth: 1, borderColor: 'rgba(239,68,68,0.5)',
    paddingVertical: 16, borderRadius: 12, flexDirection: 'row', justifyContent: 'center', alignItems: 'center', gap: 8,
  },
  denyText: { color: '#ef4444', fontWeight: 'bold', fontSize: 18 },
  approveBtn: {
    flex: 1, backgroundColor: '#22c55e', paddingVertical: 16,
    borderRadius: 12, flexDirection: 'row', justifyContent: 'center', alignItems: 'center', gap: 8,
  },
  approveText: { color: 'white', fontWeight: 'bold', fontSize: 18 },
});
