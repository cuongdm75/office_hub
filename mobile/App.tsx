import React, { useEffect } from 'react';
import { NavigationContainer } from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { StatusBar } from 'expo-status-bar';


// Screens
import { ConnectionScreen } from './src/screens/ConnectionScreen';
import { MainTabs } from './src/screens/MainTabs';

// Store
import { useSseStore } from './src/store/sseStore';

// Global Components
import { HitlApprovalSheet } from './src/components/HitlApprovalSheet';

const Stack = createNativeStackNavigator();

export default function App() {
  useEffect(() => {
    useSseStore.getState().initStore();
  }, []);

  return (
    <SafeAreaProvider>
      <NavigationContainer>
        <StatusBar style="light" />
        <Stack.Navigator 
          initialRouteName="Connection"
          screenOptions={{
            headerShown: false,
            animation: 'fade',
          }}
        >
          <Stack.Screen name="Connection" component={ConnectionScreen} />
          <Stack.Screen name="Home" component={MainTabs} />
        </Stack.Navigator>
        
        {/* Global Modal rendered above screens */}
        <HitlApprovalSheet />
      </NavigationContainer>
    </SafeAreaProvider>
  );
}
