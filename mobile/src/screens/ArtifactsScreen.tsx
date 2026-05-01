import React, { useState, useEffect, useCallback } from 'react';
import {
  View, Text, TouchableOpacity, FlatList,
  StyleSheet, Alert, ActivityIndicator,
} from 'react-native';
import { FileText, FolderOpen, Download, Trash2, RefreshCw } from 'lucide-react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { documentDirectory, downloadAsync } from 'expo-file-system/legacy';
import * as Sharing from 'expo-sharing';
import { useSseStore } from '../store/sseStore';

interface Artifact {
  id: string;
  name: string;
  url: string;      // full download URL
  timestamp: string;
  size: number;
}

export function ArtifactsScreen() {
  // currentBaseUrl is already normalised to http://host:port by sseStore
  const { currentBaseUrl, token } = useSseStore();
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [loading, setLoading] = useState(false);

  // ── Build authorised fetch headers ───────────────────────────────────────
  const authHeaders = useCallback((): HeadersInit => ({
    'Authorization': `Bearer ${token}`,
    'Content-Type': 'application/json',
  }), [token]);

  // ── Load artifacts from /api/v1/artifacts ────────────────────────────────
  const loadArtifacts = useCallback(async () => {
    if (!currentBaseUrl) return;
    setLoading(true);
    try {
      const res = await fetch(`${currentBaseUrl}/api/v1/artifacts`, {
        headers: authHeaders(),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json();
      if (json.artifacts) {
        // Rewrite relative URLs to absolute using currentBaseUrl
        const items: Artifact[] = json.artifacts.map((a: any) => ({
          ...a,
          url: a.url.startsWith('http')
            ? a.url
            : `${currentBaseUrl}${a.url}`,
        }));
        setArtifacts(items);
      }
    } catch (e) {
      console.warn('Failed to fetch artifacts', e);
    } finally {
      setLoading(false);
    }
  }, [currentBaseUrl, authHeaders]);

  useEffect(() => { loadArtifacts(); }, [loadArtifacts]);

  // ── Download & share ─────────────────────────────────────────────────────
  const handleDownload = useCallback(async (artifact: Artifact) => {
    try {
      const fileUri = (documentDirectory ?? '') + artifact.name;
      const { uri } = await downloadAsync(
        artifact.url,
        fileUri,
        { headers: { 'Authorization': `Bearer ${token}` } as any },
      );
      await Sharing.shareAsync(uri);
    } catch (err) {
      console.warn('Download failed', err);
      Alert.alert('Error', 'Failed to download or share the file.');
    }
  }, [token]);

  // ── Delete artifact ──────────────────────────────────────────────────────
  const handleDelete = useCallback((artifact: Artifact) => {
    Alert.alert(
      'Delete File',
      `Are you sure you want to delete "${artifact.name}"?`,
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Delete',
          style: 'destructive',
          onPress: async () => {
            if (!currentBaseUrl) return;
            try {
              const res = await fetch(
                `${currentBaseUrl}/api/v1/artifacts/${encodeURIComponent(artifact.id)}`,
                { method: 'DELETE', headers: authHeaders() },
              );
              if (res.ok) {
                setArtifacts(prev => prev.filter(a => a.id !== artifact.id));
              } else {
                Alert.alert('Error', 'Server refused to delete the file.');
              }
            } catch {
              Alert.alert('Error', 'Network error while deleting.');
            }
          },
        },
      ],
    );
  }, [currentBaseUrl, authHeaders]);

  const formatDate = (iso: string) => {
    try { return new Date(iso).toLocaleString(); } catch { return iso; }
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <SafeAreaView style={s.root} edges={['top']}>
      {/* Header */}
      <View style={s.header}>
        <View>
          <Text style={s.headerTitle}>Artifacts</Text>
          <Text style={s.headerSub}>FILES FROM SERVER</Text>
        </View>
        <TouchableOpacity onPress={loadArtifacts} style={s.refreshBtn} disabled={loading}>
          {loading
            ? <ActivityIndicator size="small" color="#3b82f6" />
            : <RefreshCw color="#94a3b8" size={20} />}
        </TouchableOpacity>
      </View>

      {/* Not connected notice */}
      {!currentBaseUrl && (
        <View style={s.noticeBox}>
          <Text style={s.noticeText}>Not connected — connect to a server to view artifacts.</Text>
        </View>
      )}

      <FlatList
        data={artifacts}
        keyExtractor={item => item.id}
        contentContainerStyle={s.list}
        refreshing={loading}
        onRefresh={loadArtifacts}
        ListEmptyComponent={() =>
          !loading ? (
            <View style={s.empty}>
              <FolderOpen color="#334155" size={48} />
              <Text style={s.emptyTitle}>No artifacts yet</Text>
              <Text style={s.emptySub}>Files created by the AI will appear here.</Text>
            </View>
          ) : null
        }
        renderItem={({ item }) => (
          <View style={s.item}>
            <View style={s.itemIcon}><FileText color="#3b82f6" size={20} /></View>
            <View style={s.itemInfo}>
              <Text style={s.itemName} numberOfLines={1}>{item.name}</Text>
              <Text style={s.itemMeta}>{formatDate(item.timestamp)}  ·  {formatSize(item.size)}</Text>
            </View>
            <TouchableOpacity style={s.actionBtn} onPress={() => handleDownload(item)}>
              <Download color="#3b82f6" size={18} />
            </TouchableOpacity>
            <TouchableOpacity style={s.actionBtn} onPress={() => handleDelete(item)}>
              <Trash2 color="#ef4444" size={18} />
            </TouchableOpacity>
          </View>
        )}
      />
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
  refreshBtn: { padding: 8 },
  noticeBox: {
    margin: 16, padding: 16, borderRadius: 12,
    backgroundColor: 'rgba(245,158,11,0.08)', borderWidth: 1, borderColor: 'rgba(245,158,11,0.2)',
  },
  noticeText: { color: '#fbbf24', fontSize: 13 },
  list: { padding: 16, paddingBottom: 32 },
  empty: { alignItems: 'center', justifyContent: 'center', paddingVertical: 80 },
  emptyTitle: { color: '#94a3b8', marginTop: 16, fontWeight: '500', fontSize: 16 },
  emptySub: { color: '#64748b', fontSize: 13, marginTop: 6, textAlign: 'center' },
  item: {
    flexDirection: 'row', alignItems: 'center',
    backgroundColor: '#1e293b', padding: 16,
    borderRadius: 12, marginBottom: 10,
    borderWidth: 1, borderColor: '#334155',
  },
  itemIcon: {
    width: 40, height: 40, backgroundColor: 'rgba(59,130,246,0.1)',
    borderRadius: 8, alignItems: 'center', justifyContent: 'center', marginRight: 14,
  },
  itemInfo: { flex: 1 },
  itemName: { color: 'white', fontWeight: '500', fontSize: 14, marginBottom: 4 },
  itemMeta: { color: '#64748b', fontSize: 12 },
  actionBtn: { padding: 8, marginLeft: 2 },
});
