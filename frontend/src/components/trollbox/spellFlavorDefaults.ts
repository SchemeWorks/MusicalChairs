export const SPELL_FLAVOR_DEFAULTS = {
  'spellFlavor.success': [
    "The fund smiles upon you.",
    "Clean hit. Charles would be proud.",
    "Flawless execution. You're a natural.",
    "They never saw it coming.",
    "That's how it's done in this business.",
  ],
  'spellFlavor.fail': [
    "The universe said no.",
    "Not your day. It happens to everyone. Mostly to you.",
    "Swing and a miss. The PP is still gone, though.",
    "Nothing happened. Except you're poorer now.",
    "Better luck next time. Or not. Who knows.",
  ],
  'spellFlavor.backfire': [
    "Oh no. It hit you instead.",
    "Karma works fast around here.",
    "You played yourself. Literally.",
    "That's what they call a learning experience.",
    "Charles is laughing somewhere.",
  ],
} as const;

export type SpellFlavorKey = keyof typeof SPELL_FLAVOR_DEFAULTS;
