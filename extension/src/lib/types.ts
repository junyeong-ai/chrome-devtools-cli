export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface A11yNodeInfo {
  uid: string;
  role: string;
  name?: string;
  children?: A11yNodeInfo[];
}

export interface ElementInfo {
  uid: string;
  selector: string;
  xpath?: string;
  tagName: string;
  boundingBox: Rect;
  text?: string;
  attributes?: Record<string, string>;
  computed?: {
    display: string;
    visibility: string;
    isVisible: boolean;
  };
  a11y?: {
    role: string;
    name?: string;
  };
}

export type SelectionMode = 'single' | 'multiple';

export type NavigationType = 'load' | 'pushState' | 'popState' | 'replaceState';
