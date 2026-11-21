/** Compatibility surface: DSH delivery execution belongs to its Host Adapter. */
export { createDeliveryGateTool } from '../delta-code-adapter/providers/dsh/delivery-gate.js'
export { remainingDeliveryTasks, validateTaskAcceptance } from './delivery-policy.js'
export type { DeliveryTask, TaskAcceptance } from './delivery-policy.js'
