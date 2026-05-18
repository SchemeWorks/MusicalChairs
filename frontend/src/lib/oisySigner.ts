import { Signer } from '@icp-sdk/signer';
import { PostMessageTransport } from '@icp-sdk/signer/web';
import { SignerAgent } from '@icp-sdk/signer/agent';
import { Actor } from '@icp-sdk/core/agent';
import type { Principal } from '@dfinity/principal';

// We migrated from @slide-computer/signer to @icp-sdk/signer (DFINITY's official
// successor, recommended by Thomas Gladdines after the previous lib stopped working
// with Oisy). The actor here uses @icp-sdk/core/agent's Actor (not @dfinity/agent's)
// so the Actor and SignerAgent share the same internal Certificate/polling impl —
// mixing the two would surface as "Cannot read properties of undefined (reading '_arr')".
const oisySigner = new Signer({
  transport: new PostMessageTransport({
    url: 'https://oisy.com/sign',
    windowOpenerFeatures: 'toolbar=0,location=0,menubar=0,width=525,height=705',
  }),
});

let cachedAgent: any = null;
let cachedPrincipalText: string | null = null;
let inFlightCreate: Promise<any> | null = null;

export async function getOisySignerAgent(principal: Principal): Promise<any> {
  const principalText = principal.toText();

  if (cachedAgent && cachedPrincipalText === principalText) {
    return cachedAgent;
  }

  if (inFlightCreate) {
    return inFlightCreate;
  }

  inFlightCreate = (async () => {
    const agent = await SignerAgent.create({
      signer: oisySigner,
      account: principal as any,
    });
    cachedAgent = agent;
    cachedPrincipalText = principalText;
    return agent;
  })();

  try {
    return await inFlightCreate;
  } finally {
    inFlightCreate = null;
  }
}

export function createOisyActor(
  canisterId: string,
  idlFactory: any,
  signerAgent: any,
): any {
  return Actor.createActor(idlFactory, {
    agent: signerAgent,
    canisterId,
  });
}

export function clearOisySigner(): void {
  cachedAgent = null;
  cachedPrincipalText = null;
}

// Rehydrate an existing Oisy session without prompting. Returns the account
// principal text if the signer still has an active session, null otherwise.
// The signer throws "outside of click handler" if no session exists — we
// catch and return null cleanly.
export async function restoreOisySession(): Promise<string | null> {
  try {
    const accounts = await oisySigner.getAccounts();
    if (!accounts || accounts.length === 0) return null;
    return accounts[0].owner.toText();
  } catch {
    return null;
  }
}

export { oisySigner };
