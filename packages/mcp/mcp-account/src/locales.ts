/** The copy every connector's account section shares; each connector's own text comes with its registration. */
import type { LocalizedText } from '@deepseek-ai/dsh-package-manifest'

/** Chinese dictionary and key source. */
const zh = {
  'status.checking': '正在检查连接',
  'status.disconnected': '未连接',
  'status.waiting-browser': '等待你在浏览器中确认',
  'status.authorizing': '正在连接',
  'status.connecting': '正在连接',
  'status.connected': '已连接',
  'status.reconnect': '需要重新连接',
  'status.unavailable': '暂时无法连接',
  'detail.reconnect.expired': '授权已过期，请重新连接。',
  'detail.reconnect.binding': '授权与当前设置不再匹配，请重新连接。',
  'detail.unavailable': '已保留现有授权，可以稍后重试。',
  hostBrowser: '授权需要在运行 DSH 的这台电脑上的浏览器中完成。',
  failed: '授权未完成，请重试',
  'action.reconnect': '重新连接',
  'action.open': '打开授权页面',
  'action.cancel': '取消',
  'action.signOut': '退出登录',
  'action.reauthorize': '重新授权',
  'action.retry': '重试',
  'badge.connected': '已连接',
  'badge.disconnected': '未连接',
  'badge.reconnect': '需要重新连接',
  'badge.unavailable': '暂时无法连接',
  'badge.connecting': '连接中',
}

/** Every key of the shared copy. */
export type McpAccountKey = keyof typeof zh

/** English dictionary. */
const en: Record<McpAccountKey, string> = {
  'status.checking': 'Checking connection',
  'status.disconnected': 'Not connected',
  'status.waiting-browser': 'Waiting for you to confirm in the browser',
  'status.authorizing': 'Connecting',
  'status.connecting': 'Connecting',
  'status.connected': 'Connected',
  'status.reconnect': 'Reconnection needed',
  'status.unavailable': 'Cannot connect right now',
  'detail.reconnect.expired': 'The authorization has expired. Connect again.',
  'detail.reconnect.binding': 'The authorization no longer matches the current settings. Connect again.',
  'detail.unavailable': 'The existing authorization is kept. Try again later.',
  hostBrowser: 'Authorization must be completed in a browser on the computer running DSH.',
  failed: 'Authorization was not completed. Try again.',
  'action.reconnect': 'Connect again',
  'action.open': 'Open authorization page',
  'action.cancel': 'Cancel',
  'action.signOut': 'Sign out',
  'action.reauthorize': 'Authorize again',
  'action.retry': 'Retry',
  'badge.connected': 'Connected',
  'badge.disconnected': 'Not connected',
  'badge.reconnect': 'Reconnect',
  'badge.unavailable': 'Unavailable',
  'badge.connecting': 'Connecting',
}

/**
 * The shared copy of one key in every shipped language.
 * @param key - the dictionary key.
 * @returns the text each browser resolves in its active language.
 */
export function text(key: McpAccountKey): LocalizedText {
  return { en: en[key], zh: zh[key] }
}
