import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import ReactECharts from 'echarts-for-react';

interface ChartRenderPayload {
  chart_type: string;
  title: string;
  data: any[];
  x_key: string;
  y_key: string;
  theme?: string;
}

interface ChartRenderEvent {
  request_id: string;
  payload: ChartRenderPayload;
}

export function ChartRenderer() {
  const [currentRender, setCurrentRender] = useState<ChartRenderEvent | null>(null);
  const chartRef = useRef<ReactECharts>(null);

  useEffect(() => {
    console.log('ChartRenderer: Listening for mcp_chart_render_request...');
    const unlisten = listen<ChartRenderEvent>('mcp_chart_render_request', (event) => {
      console.log('ChartRenderer: Received render request:', event.payload);
      setCurrentRender(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // When a new chart is set to render, wait a bit for animation/render to finish
  // then capture the base64 and send back to backend.
  useEffect(() => {
    if (currentRender && chartRef.current) {
      // Wait for ECharts to finish rendering
      const timer = setTimeout(async () => {
        try {
          const echartInstance = chartRef.current?.getEchartsInstance();
          if (echartInstance) {
            const base64Image = echartInstance.getDataURL({
              type: 'png',
              pixelRatio: 2, // High resolution for PPTX
              backgroundColor: currentRender.payload.theme === 'dark' ? '#1f2937' : '#ffffff'
            });

            console.log(`ChartRenderer: Sending back image for ${currentRender.request_id}`);
            await invoke('submit_chart_render', {
              requestId: currentRender.request_id,
              base64Image: base64Image
            });
            
            // Clear current render to hide the chart
            setCurrentRender(null);
          }
        } catch (error) {
          console.error('ChartRenderer: Error rendering chart to image', error);
        }
      }, 1000); // 1 second delay to ensure animations finish

      return () => clearTimeout(timer);
    }
  }, [currentRender]);

  if (!currentRender) return null;

  // Convert generic payload to ECharts options
  const { chart_type, title, data, x_key, y_key, theme } = currentRender.payload;
  
  // Transform data
  const xAxisData = data.map((item) => item[x_key] || '');
  const yAxisData = data.map((item) => item[y_key] || 0);

  // Default color palette
  const colors = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6'];

  const seriesObj: any = {
    data: yAxisData,
    type: chart_type === 'bar' ? 'bar' : chart_type === 'pie' ? 'pie' : chart_type === 'scatter' ? 'scatter' : 'line',
    smooth: chart_type === 'line',
  };

  // If pie chart, the data format needs to be different [{name, value}]
  if (chart_type === 'pie') {
    seriesObj.data = data.map(item => ({
      name: item[x_key] || '',
      value: item[y_key] || 0
    }));
    seriesObj.radius = '50%';
  }

  const option = {
    title: {
      text: title,
      left: 'center',
      textStyle: {
        color: theme === 'dark' ? '#f3f4f6' : '#111827',
        fontFamily: 'Inter, sans-serif'
      }
    },
    tooltip: {
      trigger: chart_type === 'pie' ? 'item' : 'axis'
    },
    xAxis: chart_type === 'pie' ? undefined : {
      type: 'category',
      data: xAxisData,
      axisLabel: { color: theme === 'dark' ? '#9ca3af' : '#4b5563' }
    },
    yAxis: chart_type === 'pie' ? undefined : {
      type: 'value',
      axisLabel: { color: theme === 'dark' ? '#9ca3af' : '#4b5563' }
    },
    series: [seriesObj],
    color: colors,
    animation: false, // Disable animation to ensure instant rendering capture
  };

  return (
    <div 
      style={{ 
        position: 'fixed', 
        top: -9999, 
        left: -9999, 
        width: 800, 
        height: 600,
        pointerEvents: 'none',
        zIndex: -1
      }}
    >
      <ReactECharts 
        ref={chartRef} 
        option={option} 
        style={{ width: '100%', height: '100%' }} 
        theme={theme === 'dark' ? 'dark' : 'light'}
      />
    </div>
  );
}
