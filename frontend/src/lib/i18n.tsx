import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import english from '../../../lang/English.json';
import { request } from './api';
type Dictionary = { [key: string]: string | string[] | Dictionary };
type Translate = (key: string, parameters?: Record<string, string | number>) => string;
function lookup(dictionary: Dictionary, key: string): string | undefined {
  let value: string | string[] | Dictionary | undefined = dictionary;
  for (const part of key.split('.'))
    value = value && typeof value === 'object' && !Array.isArray(value) ? value[part] : undefined;
  return typeof value === 'string' ? value : undefined;
}
// Legacy messages contain decorative emoji; keep prose and strip only presentation symbols.
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
export function I18n({
  language,
  messages,
  children,
}: {
  language?: string;
  messages?: Dictionary;
  children: ReactNode;
}) {
  const [dictionary, setDictionary] = useState<Dictionary>(english);
  useEffect(() => {
    if (!language) return;
    const controller = new AbortController();
    request<Dictionary>('/api/translations', undefined, 'GET', controller.signal)
      .then(setDictionary)
      .catch(() => {});
    document.documentElement.lang =
      (
        {
          English: 'en',
          Deutsch: 'de',
          Dansk: 'da',
          Español: 'es',
          Français: 'fr',
          Indonesian: 'id',
          Italiano: 'it',
          Magyar: 'hu',
          Nederlandse: 'nl',
          Polski: 'pl',
          Português: 'pt',
          Română: 'ro',
          Türkçe: 'tr',
          Čeština: 'cs',
          Русский: 'ru',
          Українська: 'uk',
          العربية: 'ar',
          日本語: 'ja',
          简体中文: 'zh-Hans',
          繁體中文: 'zh-Hant',
        } as Record<string, string>
      )[language] ?? 'und';
    document.documentElement.dir = language === 'العربية' ? 'rtl' : 'ltr';
    return () => controller.abort();
  }, [language]);
  return <Context value={translator(messages ?? dictionary)}>{children}</Context>;
}
export const useT = () => useContext(Context);
