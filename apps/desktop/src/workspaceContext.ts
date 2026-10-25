import { createContext, useContext } from "react";

/** The folder the open conversation works in, or null when it has none. Approval cards read it:
 *  a rule for "this workspace" can only be offered where there is one. */
export const WorkspaceContext = createContext<string | null>(null);

export const useWorkspace = () => useContext(WorkspaceContext);
