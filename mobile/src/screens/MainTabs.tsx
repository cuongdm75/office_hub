import React, { useEffect } from 'react';
import { Alert } from 'react-native';
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs';
import { MessageSquare, Activity, CheckCircle, FolderOpen, Settings } from 'lucide-react-native';
import { useSseStore } from '../store/sseStore';

import { ChatScreen } from './ChatScreen';
import { ProgressScreen } from './ProgressScreen';
import { ApprovalsScreen } from './HomeScreen';
import { ArtifactsScreen } from './ArtifactsScreen';
import { SettingsScreen } from './SettingsScreen';

const Tab = createBottomTabNavigator();

export function MainTabs({ navigation }: any) {
  const { isConnected, isConnecting, error, clearError, connect, baseUrls, token, activeWorkspaceId } = useSseStore();

  // Navigate back to Connection screen only when fully disconnected and not reconnecting
  useEffect(() => {
    if (!isConnected && !isConnecting && !error) {
      navigation.reset({ index: 0, routes: [{ name: 'Connection' }] });
    }
  }, [isConnected, isConnecting, error, navigation]);

  // Show alert when max reconnect attempts reached
  useEffect(() => {
    if (error) {
      Alert.alert(
        'Connection Lost',
        error,
        [
          {
            text: 'Retry',
            onPress: () => {
              clearError();
              if (baseUrls.length > 0) {
                connect(baseUrls, token);
              } else {
                navigation.reset({ index: 0, routes: [{ name: 'Connection' }] });
              }
            },
          },
          {
            text: 'Go to Settings',
            style: 'cancel',
            onPress: () => {
              clearError();
              navigation.reset({ index: 0, routes: [{ name: 'Connection' }] });
            },
          },
        ],
        { cancelable: false }
      );
    }
  }, [error]);

  return (
    <Tab.Navigator
      screenOptions={{
        headerShown: false,
        tabBarStyle: {
          backgroundColor: '#1e293b', // slate-800
          borderTopColor: '#334155',  // slate-700
          paddingBottom: 5,
          paddingTop: 5,
          height: 60,
        },
        tabBarActiveTintColor: '#3b82f6', // blue-500
        tabBarInactiveTintColor: '#94a3b8', // slate-400
      }}
    >
      <Tab.Screen 
        name="Chat" 
        component={ChatScreen} 
        options={{
          tabBarIcon: ({ color, size }) => <MessageSquare color={color} size={size} />
        }}
      />
      <Tab.Screen 
        name="Artifacts" 
        component={ArtifactsScreen} 
        options={{
          tabBarIcon: ({ color, size }) => <FolderOpen color={color} size={size} />
        }}
      />
      {activeWorkspaceId === 'default' || activeWorkspaceId === 'Global' ? (
        <>
          <Tab.Screen 
            name="Progress" 
            component={ProgressScreen} 
            options={{
              tabBarIcon: ({ color, size }) => <Activity color={color} size={size} />
            }}
          />
          <Tab.Screen 
            name="Approvals" 
            component={ApprovalsScreen} 
            options={{
              tabBarIcon: ({ color, size }) => <CheckCircle color={color} size={size} />
            }}
          />
          <Tab.Screen 
            name="Settings" 
            component={SettingsScreen} 
            options={{
              tabBarIcon: ({ color, size }) => <Settings color={color} size={size} />
            }}
          />
        </>
      ) : null}
    </Tab.Navigator>
  );
}
