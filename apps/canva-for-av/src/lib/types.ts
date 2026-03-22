export interface Presentation {
  id: string;
  title: string;
  url: string;
  width: number;
  height: number;
  slides: Slide[];
  fonts: FontRef[];
  imported_at: string;
}

export interface Slide {
  index: number;
  elements: SlideElement[];
  thumbnail_url: string | null;
  thumbnail_local: string | null;
}

export type SlideElement =
  | {
      type: "Text";
      content: string;
      x: number;
      y: number;
      width: number;
      height: number;
      font_family: string | null;
      font_size: number | null;
      color: string | null;
      bold: boolean;
      italic: boolean;
      rotation: number;
    }
  | {
      type: "Image";
      x: number;
      y: number;
      width: number;
      height: number;
      asset_url: string;
      local_path: string | null;
      rotation: number;
    }
  | {
      type: "Shape";
      x: number;
      y: number;
      width: number;
      height: number;
      svg_path: string | null;
      fill_color: string | null;
      stroke_color: string | null;
      rotation: number;
    };

export interface FontRef {
  family: string;
  woff2_url: string | null;
  local_path: string | null;
}

export interface RecentEntry {
  id: string;
  title: string;
  url: string;
  imported_at: string;
  slide_count: number;
  thumbnail_path: string | null;
}

export interface ImportProgress {
  stage: "Fetching" | "Parsing" | "Downloading" | "Complete" | "Failed";
  detail: string;
}
