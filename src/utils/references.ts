import type { Piece } from "../types";

export interface ParsedReference {
  name: string;
  start: number;
  end: number;
  valid: boolean;
  pieceId?: string;
}

/**
 * Parse @references from prompt text.
 * Matches @PieceName where PieceName is alphanumeric + spaces/hyphens/underscores.
 */
export function parseReferences(
  text: string,
  pieces: Piece[],
): ParsedReference[] {
  const regex = /@([\w][\w\s\-]*)/g;
  const refs: ParsedReference[] = [];
  let match;

  while ((match = regex.exec(text)) !== null) {
    const name = match[1].trim();
    const piece = pieces.find(
      (p) => p.name.toLowerCase() === name.toLowerCase(),
    );
    refs.push({
      name,
      start: match.index,
      end: match.index + match[0].length,
      valid: !!piece,
      pieceId: piece?.id,
    });
  }

  return refs;
}

/**
 * Get autocomplete suggestions for a partial @reference.
 */
export function getReferenceSuggestions(
  partial: string,
  pieces: Piece[],
): Piece[] {
  const lower = partial.toLowerCase();
  return pieces.filter((p) => p.name.toLowerCase().startsWith(lower));
}
