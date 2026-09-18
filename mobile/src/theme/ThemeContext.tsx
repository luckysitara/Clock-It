import React, { createContext, useContext, useState } from 'react';

export type ThemeMode = 'dark' | 'light';

export interface ThemeColors {
  isDark: boolean;
  background: string;
  card: string;
  cardAlt: string;
  cardBorder: string;
  primary: string;
  primaryText: string;
  text: string;
  textSecondary: string;
  textMuted: string;
  accent: string;
  accentLight: string;
  danger: string;
  warning: string;
  badgeBg: string;
  badgeBorder: string;
  inputBg: string;
  inputBorder: string;
  divider: string;
}

const darkColors: ThemeColors = {
  isDark: true,
  background: '#08090C',
  card: '#12151C',
  cardAlt: '#191E27',
  cardBorder: '#222836',
  primary: '#14F195',
  primaryText: '#03170E',
  text: '#F8FAFC',
  textSecondary: '#94A3B8',
  textMuted: '#64748B',
  accent: '#9945FF',
  accentLight: '#C084FC',
  danger: '#EF4444',
  warning: '#F59E0B',
  badgeBg: 'rgba(20, 241, 149, 0.10)',
  badgeBorder: 'rgba(20, 241, 149, 0.25)',
  inputBg: '#12151C',
  inputBorder: '#222836',
  divider: '#1E2430',
};

const lightColors: ThemeColors = {
  isDark: false,
  background: '#F8FAFC',
  card: '#FFFFFF',
  cardAlt: '#F1F5F9',
  cardBorder: '#E2E8F0',
  primary: '#059669',
  primaryText: '#FFFFFF',
  text: '#0F172A',
  textSecondary: '#475569',
  textMuted: '#94A3B8',
  accent: '#7C3AED',
  accentLight: '#8B5CF6',
  danger: '#DC2626',
  warning: '#D97706',
  badgeBg: 'rgba(5, 150, 105, 0.1)',
  badgeBorder: 'rgba(5, 150, 105, 0.25)',
  inputBg: '#F8FAFC',
  inputBorder: '#CBD5E1',
  divider: '#E2E8F0',
};

interface ThemeContextType {
  mode: ThemeMode;
  colors: ThemeColors;
  toggleTheme: () => void;
  setTheme: (mode: ThemeMode) => void;
}

const ThemeContext = createContext<ThemeContextType>({
  mode: 'light',
  colors: lightColors,
  toggleTheme: () => {},
  setTheme: () => {},
});

export const ThemeProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [mode, setMode] = useState<ThemeMode>('light');

  const toggleTheme = () => {
    setMode((prev) => (prev === 'dark' ? 'light' : 'dark'));
  };

  const colors = mode === 'dark' ? darkColors : lightColors;

  return (
    <ThemeContext.Provider value={{ mode, colors, toggleTheme, setTheme: setMode }}>
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => useContext(ThemeContext);
