import type { Messages } from "./locales";

export interface Announcement {
  id: string;
  messageKey: keyof Messages;
  link?: string;
  date: string;
  active: boolean;
}

export const ANNOUNCEMENTS: Announcement[] = [
  {
    id: "vote-community-group",
    messageKey: "announcement.1",
    link: "/vote",
    date: "2026-02-16",
    active: true,
  },
];
