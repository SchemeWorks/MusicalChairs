import { Signer } from '@slide-computer/signer';
import { SignerAgent } from '@slide-computer/signer-agent';
import { PostMessageTransport } from '@slide-computer/signer-web';
import { Actor } from '@dfinity/agent';
import type { Principal } from '@dfinity/principal';

const oisySigner = new Signer({
  transport: new PostMessageTransport({
    url: 'https://oisy.com/sign',
    windowOpenerFeatures: 'toolbar=0,location=0,menubar=0,width=525,height=705',
  }),
});

let cachedAgent: any = null;
let cachedPrincipalText: string | null = null;

export async function getOisySignerAgent(principal: Principal): Promise<any> {
  const principalText = principal.toText();

  if (cachedAgent && cachedPrincipalText === principalText) {
    return cachedAgent;
  }

  // Let SignerAgent create its own internal HttpAgent.
  // Passing @dfinity/agent's HttpAgent causes rootKey type mismatch
  // (ArrayBuffer vs Uint8Array) which breaks certificate validation.
  cachedAgent = await SignerAgent.create({
    signer: oisySigner as any,
    account: principal as any,
  });

  cachedPrincipalText = principalText;
  return cachedAgent;
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

// Rehydrate an existing Oisy session without triggering the approval popup.
// Returns the account principal text if the signer's session is still valid, null otherwise.
// @slide-computer/signer throws "outside of click handler" if no active session
// exists — we catch that and return null cleanly.
export async function restoreOisySession(): Promise<string | null> {
  try {
    const accounts = await oisySigner.accounts();
    if (!accounts || accounts.length === 0) return null;
    return accounts[0].owner.toText();
  } catch {
    return null;
  }
}

export { oisySigner };
