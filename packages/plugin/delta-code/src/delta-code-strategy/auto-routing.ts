/** Stable strategy facade: Host adapters may inject reviewed declarative policies. */
export { routeCodingTask, DEFAULT_CODING_ROUTE_RULES } from './task-routing.js'
export type { CodingRoute, CodingRouteRule } from './task-routing.js'
export { chooseTaskResources, taskResourceGuidance, DEFAULT_TASK_RESOURCE_RULES } from './task-resource-policy.js'
export type { TaskResource, TaskResourceRule } from './task-resource-policy.js'
export { registerAutomaticCodingRouter } from '../delta-code-adapter/providers/dsh/auto-routing.js'
export type { DeltaCodeRoutingPolicy } from '../delta-code-adapter/providers/dsh/auto-routing.js'
