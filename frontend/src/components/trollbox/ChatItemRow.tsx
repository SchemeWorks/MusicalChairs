import React from 'react';
import type { ChatItem, ShenaniganRecord } from '../../declarations/shenanigans/shenanigans.did';
import UserMessageRow from './rows/UserMessageRow';
import SpellRow from './rows/SpellRow';
import SignupRow from './rows/SignupRow';
import RankUpRow from './rows/RankUpRow';
import RoundResultRow from './rows/RoundResultRow';
import ReginaldRow from './rows/ReginaldRow';

interface Props {
  item: ChatItem;
  currentUserName?: string;
  spellLookup: Map<string, ShenaniganRecord>;
  isAdmin: boolean;
  onBlock: (principalText: string) => void;
  onReact: (itemId: bigint) => void;
  onDelete: (itemId: bigint) => void;
  blocked: string[];
}

export default function ChatItemRow(props: Props) {
  const { item } = props;
  if ('userMessage' in item.kind) return <UserMessageRow {...props} />;
  if ('spellCast' in item.kind) return <SpellRow item={item} spellLookup={props.spellLookup} />;
  if ('signup' in item.kind) return <SignupRow item={item} />;
  if ('rankUp' in item.kind) return <RankUpRow item={item} />;
  if ('roundResult' in item.kind) return <RoundResultRow item={item} />;
  if ('reginald' in item.kind) return <ReginaldRow item={item} />;
  if ('pinUpdate' in item.kind) return null; // Never rendered inline.
  return null;
}
