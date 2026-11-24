import type { SidebarBrandMarkOwnerProps } from '@deepseek-ai/dsh-client-ui-sidebar/client'
import deltaLogo from './assets/delta-logo-256x256.png'

/**
 * Render Delta's own mark with the presentation requested by its host surface.
 * @param props - Host-supplied mark presentation.
 * @returns the Delta mark.
 */
export function OfficialBrandMark({ size }: SidebarBrandMarkOwnerProps) {
  return <img src={deltaLogo} alt="" width={size} height={size} draggable={false} />
}

/**
 * Render the Delta product name beside its independently slotted mark.
 * @returns the official name wordmark.
 */
export function OfficialBrandName() {
  return 'Delta'
}
