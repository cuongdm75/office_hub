import React, { useEffect, useRef } from 'react';
import { View, Text, ScrollView, ActivityIndicator, StyleSheet, Animated } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Bot, CheckCircle } from 'lucide-react-native';
import { useSseStore } from '../store/sseStore';

const IndeterminateProgressBar = React.memo(() => {
  const animatedValue = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Animated.loop(
      Animated.sequence([
        Animated.timing(animatedValue, {
          toValue: 1,
          duration: 1000,
          useNativeDriver: false,
        }),
        Animated.timing(animatedValue, {
          toValue: 0,
          duration: 1000,
          useNativeDriver: false,
        }),
      ])
    ).start();
  }, []);

  const width = animatedValue.interpolate({
    inputRange: [0, 1],
    outputRange: ['10%', '90%'],
  });
  
  const translateX = animatedValue.interpolate({
    inputRange: [0, 1],
    outputRange: [0, 200], // Approximate width, can be improved
  });

  return (
    <View style={s.progressTrack}>
      <Animated.View style={[s.progressBar, { width, transform: [{ translateX }] }]} />
    </View>
  );
});

export function ProgressScreen() {
  const { activeTasks } = useSseStore();
  const activeWorkflows = React.useMemo(() => Object.values(activeTasks), [activeTasks]);

  return (
    <SafeAreaView style={s.root} edges={['top']}>
      <View style={s.header}>
        {activeWorkflows.length > 0
          ? <ActivityIndicator size="small" color="#3b82f6" />
          : <CheckCircle size={20} color="#10b981" />}
        <Text style={s.headerTitle}>Active Progress</Text>
      </View>

      <ScrollView style={s.flex} contentContainerStyle={s.content}>
        {activeWorkflows.length > 0 ? (
          activeWorkflows.map((w, index) => (
            <View key={`${w.run_id}_${w.step_name || index}`} style={s.card}>
              <View style={s.cardHeader}>
                <View style={s.cardTitleRow}>
                  <Bot size={20} color="#3b82f6" />
                  <Text style={s.cardTitle}>{w.step_name || w.workflow_name || 'Step'}</Text>
                </View>
                <Text style={s.statusText}>{w.status}</Text>
              </View>
              <Text style={s.messageText}>Message: {w.message || 'Processing...'}</Text>
              <IndeterminateProgressBar />
              <Text style={s.idText}>ID: {w.run_id.substring(0, 8)}</Text>
            </View>
          ))
        ) : (
          <Text style={s.emptyText}>No active tasks right now.</Text>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#0f172a' },
  flex: { flex: 1 },
  header: {
    flexDirection: 'row', alignItems: 'center', padding: 16,
    backgroundColor: '#1e293b', borderBottomWidth: 1, borderBottomColor: '#334155', gap: 12,
  },
  headerTitle: { color: 'white', fontWeight: 'bold', fontSize: 20 },
  content: { padding: 16 },
  card: {
    backgroundColor: '#1e293b', borderRadius: 16, padding: 16, marginBottom: 16,
    borderWidth: 1, borderColor: 'rgba(59,130,246,0.5)',
  },
  cardHeader: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 },
  cardTitleRow: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  cardTitle: { color: 'white', fontWeight: 'bold', fontSize: 18 },
  statusText: { color: '#60a5fa', fontWeight: '500' },
  messageText: { color: '#cbd5e1', marginBottom: 8 },
  progressTrack: { height: 8, backgroundColor: '#334155', borderRadius: 4, overflow: 'hidden', marginBottom: 12 },
  progressBar: { height: '100%', backgroundColor: '#3b82f6', borderRadius: 4 },
  idText: { color: '#64748b', fontSize: 12 },
  emptyText: { color: '#94a3b8', textAlign: 'center', marginTop: 40 },
});
