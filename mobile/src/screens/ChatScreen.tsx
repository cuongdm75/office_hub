import React, { useState, useRef, useEffect } from 'react';
import {
  View, Text, TextInput, TouchableOpacity, FlatList,
  KeyboardAvoidingView, Platform, ActivityIndicator,
  Alert, Keyboard, StyleSheet, ScrollView,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Send, Bot, User, Menu, Mic, Paperclip, X, Trash2 } from 'lucide-react-native';
import * as DocumentPicker from 'expo-document-picker';
import { readAsStringAsync, writeAsStringAsync, documentDirectory, downloadAsync, uploadAsync, FileSystemUploadType } from 'expo-file-system/legacy';
import * as Sharing from 'expo-sharing';
import { useSseStore } from '../store/sseStore';
import Markdown from 'react-native-markdown-display';
import { useVoiceRecording } from '../hooks/useVoiceRecording';

const MAX_FILE_SIZE_BYTES = 10 * 1024 * 1024;

const MessageItem = React.memo(({ msg, handleDownloadAttachment, baseUrl }: {
  msg: any;
  handleDownloadAttachment: (attachment: any) => void;
  baseUrl: string;
}) => {
  // Replace office-hub://files/ URI with actual HTTP URL for Markdown rendering
  const processedText = React.useMemo(() => {
    if (!msg.text) return '';
    if (!baseUrl) return msg.text;
    return msg.text.replace(/office-hub:\/\/files\//g, `${baseUrl}/api/v1/files/download/`);
  }, [msg.text, baseUrl]);

  return (
  <View style={[s.msgRow, msg.sender === 'user' ? s.msgRowUser : s.msgRowAgent]}>
    {msg.sender === 'agent' && (
      <View style={s.avatarAgent}>
        <Bot size={16} color="white" />
      </View>
    )}
    <View style={[s.bubble, msg.sender === 'user' ? s.bubbleUser : s.bubbleAgent]}>
      {msg.sender === 'agent' && !!msg.agent_used && (
        <Text style={s.agentLabel}>{msg.agent_used}</Text>
      )}
      {msg.sender === 'user' ? (
        <Text style={s.msgTextUser}>{msg.text || ' '}</Text>
      ) : (
        <View>
          {msg.text ? (
            <Markdown style={markdownStyles}>{processedText}</Markdown>
          ) : (
            <Text style={s.processingText}>Processing...</Text>
          )}
          {(msg.metadata?.attachment || msg.metadata?.system?.attachment) && (() => {
            const attachment = msg.metadata.attachment || msg.metadata.system.attachment;
            return (
              <TouchableOpacity
                style={s.attachmentBtn}
                onPress={() => handleDownloadAttachment(attachment)}
              >
                <Paperclip size={16} color="#38bdf8" />
                <Text style={s.attachmentName} numberOfLines={1}>
                  {attachment.name}
                </Text>
              </TouchableOpacity>
            );
          })()}
        </View>
      )}
    </View>
    {msg.sender === 'user' && (
      <View style={s.avatarUser}>
        <User size={16} color="white" />
      </View>
    )}
  </View>
  );
});

export function ChatScreen({ navigation }: any) {
  const [inputText, setInputText] = useState('');
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);
  const {
    messages, llmThought, activeTasks, sendCommand, sendVoiceCommand,
    isConnected, sessions, listSessions, getSessionHistory, deleteSession,
    currentSessionId, setCurrentSessionId, currentBaseUrl: url,
    error, clearError,
    workspaces, activeWorkspaceId, listWorkspaces, setActiveWorkspaceId
  } = useSseStore();
  const flatListRef = useRef<FlatList>(null);
  const { isRecording, startRecording, stopRecording } = useVoiceRecording();
  const [attachedFile, setAttachedFile] = useState<{ name: string; base64?: string; file_path?: string; size: number } | null>(null);
  const [isPickingFile, setIsPickingFile] = useState(false);
  const [keyboardHeight, setKeyboardHeight] = useState(0);

  // Mention states
  const { fetchWorkspaceFiles } = useSseStore();
  const [showMention, setShowMention] = useState(false);
  const [mentionQuery, setMentionQuery] = useState('');
  const [workspaceFiles, setWorkspaceFiles] = useState<any[]>([]);
  const [mentionCursorIndex, setMentionCursorIndex] = useState(-1);

  useEffect(() => {
    if (activeWorkspaceId && activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global') {
      fetchWorkspaceFiles(activeWorkspaceId).then(files => setWorkspaceFiles(files)).catch(() => setWorkspaceFiles([]));
    } else {
      setWorkspaceFiles([]);
    }
  }, [activeWorkspaceId]);

  useEffect(() => {
    if (Platform.OS !== 'android') return;
    const show = Keyboard.addListener('keyboardDidShow', (e) => setKeyboardHeight(e.endCoordinates.height));
    const hide = Keyboard.addListener('keyboardDidHide', () => setKeyboardHeight(0));
    return () => { show.remove(); hide.remove(); };
  }, []);

  useEffect(() => {
    if (messages.length > 0 || llmThought) {
      setTimeout(() => flatListRef.current?.scrollToEnd({ animated: true }), 100);
    }
  }, [messages.length, llmThought]);

  useEffect(() => {
    if (isSidebarOpen && isConnected) {
      listSessions();
      listWorkspaces();
    }
  }, [isSidebarOpen, isConnected, listSessions, listWorkspaces]);

  const getHttpUploadUrl = () => {
    if (!url) return null;
    // url is already an http base URL in sseStore (e.g. http://192.168.1.x:9002)
    return `${url}/api/v1/files/upload`;
  };

  const handleAttachFile = async () => {
    try {
      const result = await DocumentPicker.getDocumentAsync({ copyToCacheDirectory: true });
      if (result.canceled || !result.assets?.length) return;
      const file = result.assets[0];
      if (file.size && file.size > MAX_FILE_SIZE_BYTES) {
        Alert.alert('File too large', 'Maximum file size is 10MB.');
        return;
      }
      setIsPickingFile(true);

      const uploadUrl = getHttpUploadUrl();
      if (uploadUrl) {
        const uploadResult = await uploadAsync(uploadUrl, file.uri, {
          httpMethod: 'POST',
          uploadType: FileSystemUploadType.MULTIPART,
          fieldName: 'file',
        });
        
        const responseData = JSON.parse(uploadResult.body);
        if (responseData.resource?.uri || responseData.file_path) {
          const finalPath = responseData.resource?.uri || responseData.file_path;
          setAttachedFile({ name: file.name, file_path: finalPath, size: file.size || 0 });
        } else {
          Alert.alert('Upload Error', responseData.error || 'Failed to upload file.');
        }
      } else {
        const base64 = await readAsStringAsync(file.uri, { encoding: 'base64' });
        setAttachedFile({ name: file.name, base64, size: file.size || 0 });
      }
    } catch (err) {
      Alert.alert('Error', 'Failed to upload or read the selected file.');
    } finally {
      setIsPickingFile(false);
    }
  };

  const handleDownloadAttachment = React.useCallback(async (attachment: { name: string; base64?: string; url?: string }) => {
    try {
      const fileUri = (documentDirectory ?? '') + attachment.name;
      if (attachment.url) {
        let downloadUrl = attachment.url;
        if (downloadUrl.startsWith('office-hub://files/')) {
          downloadUrl = downloadUrl.replace('office-hub://files/', `${url}/api/v1/files/download/`);
        } else if (url) {
          const wsIp = url.replace('ws://', '').replace('wss://', '').split(':')[0];
          downloadUrl = downloadUrl.replace(/http:\/\/[0-9\.]+:\d+/, `http://${wsIp}:9002`);
        }
        const { uri } = await downloadAsync(downloadUrl, fileUri);
        await Sharing.shareAsync(uri);
      } else if (attachment.base64) {
        await writeAsStringAsync(fileUri, attachment.base64, { encoding: 'base64' });
        await Sharing.shareAsync(fileUri);
      }
    } catch (err) {
      Alert.alert('Error', 'Failed to open attachment.');
    }
  }, [url]);

  const handleSend = () => {
    if (!isConnected) { Alert.alert('Not connected'); return; }
    if (!inputText.trim() && !attachedFile) return;
    sendCommand(inputText.trim(), attachedFile);
    setInputText('');
    setAttachedFile(null);
  };

  const handleVoice = async () => {
    if (!isConnected) { Alert.alert('Not connected'); return; }
    if (isRecording) {
      const base64Audio = await stopRecording();
      if (base64Audio) sendVoiceCommand(base64Audio);
    } else {
      await startRecording();
    }
  };

  const canSend = (!!inputText.trim() || !!attachedFile) && isConnected;

  const renderMessageItem = React.useCallback(({ item: msg }: any) => (
    <MessageItem msg={msg} handleDownloadAttachment={handleDownloadAttachment} baseUrl={url} />
  ), [handleDownloadAttachment, url]);

  return (
    <SafeAreaView style={s.root} edges={['top']}>
      <KeyboardAvoidingView
        style={s.flex}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
        keyboardVerticalOffset={Platform.OS === 'ios' ? 10 : 0}
        enabled={Platform.OS === 'ios'}
      >
        {/* Header */}
        <View style={s.header}>
          <TouchableOpacity onPress={() => setIsSidebarOpen(true)}>
            <Menu color="#94a3b8" size={24} />
          </TouchableOpacity>
          <Text style={s.headerTitle}>Office Hub Assistant</Text>
          <View style={{ width: 24 }} />
        </View>

        {/* Global Error Banner */}
        {error && (
          <View style={s.errorBanner}>
            <Text style={s.errorBannerText}>{error}</Text>
            <TouchableOpacity onPress={clearError}>
              <X size={16} color="#ef4444" />
            </TouchableOpacity>
          </View>
        )}

        {/* Content shifts up on Android when keyboard opens */}
        <View style={[s.flex, Platform.OS === 'android' && { marginBottom: keyboardHeight }]}>
          <FlatList
            ref={flatListRef}
            style={s.flex}
            contentContainerStyle={s.listContent}
            keyboardShouldPersistTaps="handled"
            data={messages}
            keyExtractor={(item) => item.id}
            initialNumToRender={15}
            maxToRenderPerBatch={10}
            windowSize={5}
            removeClippedSubviews={Platform.OS === 'android'}
            onContentSizeChange={() => flatListRef.current?.scrollToEnd({ animated: true })}
            onLayout={() => flatListRef.current?.scrollToEnd({ animated: true })}
            ListEmptyComponent={() => (
              <View style={s.emptyContainer}>
                <Bot size={48} color="#475569" />
                <Text style={s.emptyText}>No messages yet. Send a command to start.</Text>
              </View>
            )}
            renderItem={renderMessageItem}
            ListFooterComponent={() => llmThought ? (
              <View style={[s.msgRow, s.msgRowAgent]}>
                <View style={s.avatarAgent}>
                  <Bot size={16} color="white" />
                </View>
                <View style={[s.bubble, s.bubbleAgent, { opacity: 0.8 }]}>
                  <Text style={[s.processingText, { marginBottom: 4 }]}>Suy nghĩ...</Text>
                  <Text style={{ fontSize: 13, color: '#64748b', fontStyle: 'italic' }}>{llmThought}</Text>
                </View>
              </View>
            ) : null}
          />

          <View style={s.inputArea}>
            {/* Mention Popup */}
            {showMention && activeWorkspaceId && activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global' && (
              <View style={s.mentionContainer}>
                <View style={s.mentionHeader}>
                  <Text style={s.mentionHeaderText}>Tài liệu trong Workspace</Text>
                </View>
                <FlatList
                  data={workspaceFiles.filter(f => f.name.toLowerCase().includes(mentionQuery))}
                  keyExtractor={(item, idx) => `${item.name}-${idx}`}
                  keyboardShouldPersistTaps="handled"
                  style={{ maxHeight: 150 }}
                  renderItem={({ item }) => (
                    <TouchableOpacity
                      style={s.mentionItem}
                      onPress={() => {
                        const before = inputText.slice(0, mentionCursorIndex);
                        const currentCursor = inputText.indexOf('@', mentionCursorIndex);
                        const after = inputText.slice(currentCursor + mentionQuery.length + 1);
                        setInputText(`${before}@${item.name} ${after}`);
                        setShowMention(false);
                      }}
                    >
                      <View style={s.mentionCategory}>
                        <Text style={s.mentionCategoryText}>{item.category || 'doc'}</Text>
                      </View>
                      <Text style={s.mentionName} numberOfLines={1}>{item.name}</Text>
                    </TouchableOpacity>
                  )}
                />
              </View>
            )}

            {/* Active tasks */}
            {Object.values(activeTasks).length > 0 && (
              <View style={s.tasksBanner}>
                {Object.values(activeTasks).map((task) => {
                  let agentName = task.workflow_name || 'System';
                  const isAgent = agentName !== 'System' && agentName !== 'global';
                  
                  return (
                    <View key={task.run_id + task.step_name} style={s.taskRow}>
                      <ActivityIndicator size="small" color="#8b5cf6" style={{ marginRight: 8 }} />
                      <View style={{ flex: 1 }}>
                        {isAgent && (
                          <Text style={{ fontSize: 11, color: '#8b5cf6', fontWeight: 'bold', textTransform: 'uppercase' }}>
                            {agentName} đang hoạt động
                          </Text>
                        )}
                        <Text style={s.taskText} numberOfLines={1}>
                          {task.message || `Đang xử lý ${task.step_name || task.workflow_name}...`}
                        </Text>
                      </View>
                    </View>
                  );
                })}
              </View>
            )}

            {/* Attached file preview */}
            {attachedFile && (
              <View style={s.filePreview}>
                <View style={s.filePreviewLeft}>
                  <Paperclip size={16} color="#94a3b8" />
                  <View style={s.filePreviewInfo}>
                    <Text style={s.fileName} numberOfLines={1}>{attachedFile.name}</Text>
                    {attachedFile.size > 0 && (
                      <Text style={s.fileSize}>{(attachedFile.size / 1024).toFixed(0)} KB</Text>
                    )}
                  </View>
                </View>
                <TouchableOpacity onPress={() => setAttachedFile(null)} style={s.fileRemoveBtn}>
                  <X size={16} color="#ef4444" />
                </TouchableOpacity>
              </View>
            )}

            {/* Input row */}
            <View style={[s.inputRow, Platform.OS === 'ios' && { paddingBottom: 24 }]}>
              <TouchableOpacity
                onPress={handleAttachFile}
                disabled={isPickingFile}
                style={s.iconBtn}
              >
                {isPickingFile
                  ? <ActivityIndicator size="small" color="#94a3b8" />
                  : <Paperclip size={20} color="#94a3b8" />}
              </TouchableOpacity>

              <TextInput
                style={s.textInput}
                placeholder="Type a command or ask a question..."
                placeholderTextColor="#64748b"
                value={inputText}
                onChangeText={(text) => {
                  setInputText(text);
                  
                  if (activeWorkspaceId && activeWorkspaceId !== 'default' && activeWorkspaceId !== 'Global') {
                    // Simple detection for last word starting with @
                    const words = text.split(/[\s\n]+/);
                    const lastWord = words[words.length - 1];
                    if (lastWord.startsWith('@')) {
                      setShowMention(true);
                      setMentionQuery(lastWord.substring(1).toLowerCase());
                      setMentionCursorIndex(text.lastIndexOf(lastWord));
                    } else {
                      setShowMention(false);
                    }
                  }
                }}
                multiline
                maxLength={500}
              />

              <TouchableOpacity
                onPress={handleSend}
                style={[s.sendBtn, canSend ? s.sendBtnActive : s.sendBtnInactive]}
              >
                <Send size={20} color={canSend ? 'white' : '#94a3b8'} />
              </TouchableOpacity>

              <TouchableOpacity
                onPress={handleVoice}
                style={[s.voiceBtn, isRecording ? s.voiceBtnActive : s.iconBtn]}
              >
                <Mic size={20} color={isRecording ? 'white' : '#94a3b8'} />
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </KeyboardAvoidingView>

      {/* Sidebar */}
      {isSidebarOpen && (
        <View style={s.sidebarOverlay}>
          <TouchableOpacity
            style={s.sidebarBackdrop}
            activeOpacity={1}
            onPress={() => setIsSidebarOpen(false)}
          />
          <View style={s.sidebarContent}>
            <View style={s.sidebarHeader}>
              <Text style={s.sidebarTitle}>Workspace & Chat</Text>
              <TouchableOpacity onPress={() => setIsSidebarOpen(false)}>
                <X size={24} color="#94a3b8" />
              </TouchableOpacity>
            </View>

            {/* Workspace Selector */}
            {workspaces.length > 0 && (
              <View style={s.workspaceContainer}>
                <Text style={s.sectionLabel}>WORKSPACE</Text>
                <ScrollView horizontal showsHorizontalScrollIndicator={false} style={{ marginBottom: 16 }}>
                  {workspaces.map(ws => (
                    <TouchableOpacity
                      key={ws.id}
                      style={[s.workspacePill, activeWorkspaceId === ws.id && s.workspacePillActive]}
                      onPress={() => setActiveWorkspaceId(ws.id)}
                    >
                      <Text style={[s.workspacePillText, activeWorkspaceId === ws.id && s.workspacePillTextActive]}>
                        {ws.name}
                      </Text>
                    </TouchableOpacity>
                  ))}
                </ScrollView>
              </View>
            )}

            <Text style={s.sectionLabel}>CHATS</Text>
            <TouchableOpacity
              style={s.newChatBtn}
              onPress={() => { setCurrentSessionId(null); setIsSidebarOpen(false); }}
            >
              <Text style={s.newChatText}>+ New Chat</Text>
            </TouchableOpacity>
            <ScrollView>
              {sessions.filter(s => {
                if (activeWorkspaceId === 'default' || activeWorkspaceId === 'Global') {
                  return !s.workspaceId || s.workspaceId === 'default';
                }
                return s.workspaceId === activeWorkspaceId;
              }).map((session) => (
                <View key={session.id} style={s.sessionItemContainer}>
                  <TouchableOpacity
                    style={[s.sessionItem, currentSessionId === session.id && s.sessionItemActive]}
                    onPress={() => {
                      setCurrentSessionId(session.id);
                      getSessionHistory(session.id);
                      setIsSidebarOpen(false);
                    }}
                  >
                    <Text style={s.sessionTitle} numberOfLines={1}>{session.title || 'Untitled'}</Text>
                    <Text style={s.sessionDate}>{new Date(session.lastActive).toLocaleDateString()}</Text>
                  </TouchableOpacity>
                  <TouchableOpacity
                    style={s.deleteSessionBtn}
                    onPress={() => {
                      Alert.alert(
                        'Delete Chat',
                        'Are you sure you want to delete this conversation?',
                        [
                          { text: 'Cancel', style: 'cancel' },
                          { 
                            text: 'Delete', 
                            style: 'destructive',
                            onPress: () => deleteSession(session.id)
                          }
                        ]
                      );
                    }}
                  >
                    <Trash2 size={16} color="#ef4444" />
                  </TouchableOpacity>
                </View>
              ))}
            </ScrollView>
          </View>
        </View>
      )}
    </SafeAreaView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#0f172a' },
  flex: { flex: 1 },
  header: {
    flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between',
    paddingHorizontal: 16, paddingVertical: 12,
    backgroundColor: '#1e293b', borderBottomWidth: 1, borderBottomColor: '#334155',
  },
  headerTitle: { color: 'white', fontWeight: 'bold', fontSize: 18 },
  errorBanner: {
    backgroundColor: '#fee2e2',
    paddingHorizontal: 16,
    paddingVertical: 10,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    borderBottomWidth: 1,
    borderBottomColor: '#fecaca',
  },
  errorBannerText: { color: '#b91c1c', fontSize: 14, flex: 1, marginRight: 8 },
  listContent: {
    padding: 16,
    paddingBottom: 24,
  },
  mentionContainer: {
    backgroundColor: '#1e293b', // slate-800
    borderTopWidth: 1,
    borderTopColor: '#334155', // slate-700
    maxHeight: 200,
  },
  mentionHeader: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    backgroundColor: '#0f172a', // slate-900
    borderBottomWidth: 1,
    borderBottomColor: '#334155',
  },
  mentionHeaderText: {
    color: '#94a3b8', // slate-400
    fontSize: 12,
    fontWeight: 'bold',
  },
  mentionItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#334155',
  },
  mentionCategory: {
    backgroundColor: '#334155',
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 4,
    marginRight: 8,
  },
  mentionCategoryText: {
    color: '#cbd5e1',
    fontSize: 10,
  },
  mentionName: {
    color: '#f8fafc', // slate-50
    fontSize: 14,
    flex: 1,
  },
  emptyContainer: { flex: 1, alignItems: 'center', justifyContent: 'center', marginTop: 80 },
  emptyText: { color: '#94a3b8', marginTop: 16, textAlign: 'center' },
  msgRow: { marginBottom: 24, flexDirection: 'row' },
  msgRowUser: { justifyContent: 'flex-end' },
  msgRowAgent: { justifyContent: 'flex-start' },
  avatarAgent: {
    backgroundColor: '#334155', borderRadius: 16, width: 32, height: 32,
    alignItems: 'center', justifyContent: 'center', marginRight: 8,
  },
  avatarUser: {
    backgroundColor: '#475569', borderRadius: 16, width: 32, height: 32,
    alignItems: 'center', justifyContent: 'center', marginLeft: 8,
  },
  bubble: { maxWidth: '85%', borderRadius: 16, padding: 12 },
  bubbleUser: { backgroundColor: '#2563eb', borderTopRightRadius: 4 },
  bubbleAgent: { backgroundColor: '#1e293b', borderTopLeftRadius: 4, borderWidth: 1, borderColor: '#334155' },
  agentLabel: { fontSize: 10, color: '#94a3b8', marginBottom: 4, textTransform: 'uppercase', letterSpacing: 1 },
  msgTextUser: { color: 'white', fontSize: 14 },
  processingText: { color: '#64748b', fontStyle: 'italic' },
  attachmentBtn: {
    marginTop: 8, flexDirection: 'row', alignItems: 'center',
    backgroundColor: '#334155', padding: 8, borderRadius: 8,
  },
  attachmentName: { color: '#38bdf8', marginLeft: 8, fontWeight: '600', flex: 1 },
  inputArea: { backgroundColor: '#1e293b', borderTopWidth: 1, borderTopColor: '#334155' },
  tasksBanner: { paddingHorizontal: 16, paddingVertical: 8, borderBottomWidth: 1, borderBottomColor: '#334155' },
  taskRow: { flexDirection: 'row', alignItems: 'center', paddingVertical: 4 },
  taskDot: { width: 6, height: 6, borderRadius: 3, backgroundColor: '#60a5fa', marginRight: 8 },
  taskText: { color: '#cbd5e1', fontSize: 12, flex: 1 },
  filePreview: {
    flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between',
    marginHorizontal: 16, marginTop: 8, padding: 8,
    backgroundColor: '#334155', borderRadius: 8,
  },
  filePreviewLeft: { flexDirection: 'row', alignItems: 'center', flex: 1 },
  filePreviewInfo: { marginLeft: 8, flex: 1 },
  fileName: { color: '#cbd5e1' },
  fileSize: { color: '#64748b', fontSize: 11 },
  fileRemoveBtn: { padding: 4 },
  inputRow: { flexDirection: 'row', alignItems: 'center', padding: 16, paddingBottom: 16 },
  iconBtn: {
    width: 40, height: 40, borderRadius: 20,
    alignItems: 'center', justifyContent: 'center',
    backgroundColor: '#334155', marginRight: 8,
  },
  textInput: {
    flex: 1, backgroundColor: '#0f172a', color: 'white',
    borderRadius: 20, paddingHorizontal: 16, paddingVertical: 10,
    marginRight: 12, borderWidth: 1, borderColor: '#334155',
    fontSize: 14, maxHeight: 100,
  },
  sendBtn: { width: 48, height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center' },
  sendBtnActive: { backgroundColor: '#2563eb' },
  sendBtnInactive: { backgroundColor: '#334155' },
  voiceBtn: { width: 48, height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center', marginLeft: 8 },
  voiceBtnActive: { backgroundColor: '#ef4444' },
  sidebarOverlay: { position: 'absolute', top: 0, left: 0, right: 0, bottom: 0, zIndex: 50, flexDirection: 'row' },
  sidebarBackdrop: { position: 'absolute', top: 0, left: 0, right: 0, bottom: 0, backgroundColor: 'rgba(0,0,0,0.5)' },
  sidebarContent: {
    width: '80%', height: '100%', backgroundColor: '#1e293b',
    paddingTop: 48, paddingBottom: 24, paddingHorizontal: 16,
    borderRightWidth: 1, borderRightColor: '#334155',
  },
  sidebarHeader: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 24 },
  sidebarTitle: { color: 'white', fontSize: 20, fontWeight: 'bold' },
  newChatBtn: { backgroundColor: '#2563eb', borderRadius: 8, padding: 12, marginBottom: 16, alignItems: 'center' },
  newChatText: { color: 'white', fontWeight: '600' },
  sessionItemContainer: { flexDirection: 'row', alignItems: 'center', marginBottom: 4 },
  sessionItem: { padding: 12, borderRadius: 8, flex: 1 },
  sessionItemActive: { backgroundColor: '#334155' },
  sessionTitle: { color: '#e2e8f0', fontWeight: '500' },
  sessionDate: { color: '#64748b', fontSize: 11, marginTop: 2 },
  deleteSessionBtn: { padding: 12, marginLeft: 4, justifyContent: 'center', alignItems: 'center' },
  workspaceContainer: { marginBottom: 8 },
  sectionLabel: { color: '#64748b', fontSize: 12, fontWeight: 'bold', marginBottom: 8, marginTop: 8 },
  workspacePill: { 
    paddingHorizontal: 12, paddingVertical: 6, 
    borderRadius: 16, backgroundColor: '#334155', 
    marginRight: 8, borderWidth: 1, borderColor: '#475569' 
  },
  workspacePillActive: { backgroundColor: '#2563eb', borderColor: '#2563eb' },
  workspacePillText: { color: '#cbd5e1', fontSize: 13 },
  workspacePillTextActive: { color: 'white', fontWeight: 'bold' },
});

const markdownStyles: any = {
  body: { color: '#e2e8f0', fontSize: 14, lineHeight: 20 },
  code_inline: { backgroundColor: '#334155', color: '#7dd3fc', borderRadius: 4, paddingHorizontal: 4 },
  fence: { backgroundColor: '#334155', borderRadius: 8, padding: 12, marginVertical: 8 },
  code_block: { backgroundColor: '#334155', borderRadius: 8, padding: 12 },
  strong: { color: 'white', fontWeight: 'bold' },
  link: { color: '#38bdf8' },
  bullet_list: { marginVertical: 4 },
  ordered_list: { marginVertical: 4 },
};
