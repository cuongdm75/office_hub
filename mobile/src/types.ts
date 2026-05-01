export interface ApprovalAction {
  id: string;
  label: string;
  style: string;
}

export interface ApprovalRequestPayload {
  action_id: string;
  description: string;
  risk_level: string;
  payload?: any;
  timeout_seconds: number;
  actions: ApprovalAction[];
  requested_at: string;
}

export interface WorkflowStatusPayload {
  run_id: string;
  workflow_id: string;
  workflow_name: string;
  status: string;
  message?: string;
  updated_at: string;
}

export interface NotificationPayload {
  notification_id: string;
  level: 'info' | 'success' | 'warning' | 'error';
  title: string;
  body: string;
  data?: any;
  timestamp: string;
}

export type ServerMessage = 
  | { type: 'pong'; timestamp_ms: number }
  | { type: 'notification' } & NotificationPayload
  | { type: 'chat_reply'; session_id: string; content: string; intent?: string; agent_used?: string; timestamp: string }
  | { type: 'approval_request' } & ApprovalRequestPayload
  | { type: 'workflow_status' } & WorkflowStatusPayload
  | { type: 'agent_statuses'; agents: any[]; updated_at: string }
  | { type: 'error'; error_code: string; message: string; request_id?: string };

export type ClientMessage =
  | { type: 'ping'; timestamp_ms: number }
  | { type: 'auth'; token: string }
  | { type: 'command'; session_id?: string; text: string; context?: any }
  | { type: 'approval_response'; action_id: string; approved: boolean; reason?: string; responded_by: string }
  | { type: 'workflow_status_request'; workflow_id?: string }
  | { type: 'agent_status_request' }
  | { type: 'disconnect'; reason?: string };
