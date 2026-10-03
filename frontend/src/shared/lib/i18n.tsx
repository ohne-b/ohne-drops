import { createContext, useContext, type ReactNode } from 'react';
import english from '../../../../lang/English.json';
type Dictionary = { [key: string]: string | string[] | Dictionary };
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
function lookup(dictionary: Dictionary, key: string): string | undefined {
  let value: string | string[] | Dictionary | undefined = dictionary;
  for (const part of key.split('.'))
    value = value && typeof value === 'object' && !Array.isArray(value) ? value[part] : undefined;
  return typeof value === 'string' ? value : undefined;
}
// Keep log/status presentation consistent with the neutral interface.
export const plainText = (text: string) =>
  text
    .replace(/[\p{Extended_Pictographic}\uFE0F\u2713\u2714\u274C\u23F3]/gu, '')
    .replace(/ {2,}/g, ' ')
    .trim();
export function translator(dictionary: Dictionary): Translate {
  return (key, parameters = {}) => {
    const path = key.includes('.') ? key : `gui.redesign.${key}`;
    const text = lookup(dictionary, path) ?? lookup(english, path) ?? key;
    return plainText(
      text.replace(/\{(\w+)\}/g, (match: string, name: string) =>
        String(parameters[name] ?? match),
      ),
    );
  };
}
const Context = createContext<Translate>(translator(english));
export function I18n({ messages, children }: { messages?: Dictionary; children: ReactNode }) {
  return <Context value={translator(messages ?? english)}>{children}</Context>;
}
export const useT = () => useContext(Context);
