import { useEffect, useMemo, useState } from "react";
import type { ProjectSummary } from "../../lib/types";
import { apiUrl, STORAGE_PREFIX } from "./util";

interface UseProjectStateProps {
  apiBase: string;
  setStatusMessage: (msg: string) => void;
}

export function useProjectState({ apiBase, setStatusMessage }: UseProjectStateProps) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selectedProject, setSelectedProject] = useState<string>(
    () => window.localStorage.getItem(`${STORAGE_PREFIX}.selectedProject`) ?? "",
  );

  async function loadProjects() {
    try {
      const response = await fetch(apiUrl(apiBase, "/api/projects"));
      const payload = (await response.json()) as ProjectSummary[];
      setProjects(payload);
      setSelectedProject((current) => {
        if (current && payload.some((project) => project.id === current)) {
          return current;
        }
        return payload[0]?.id ?? "";
      });
      return payload;
    } catch (error) {
      setStatusMessage(String(error));
      return [];
    }
  }

  useEffect(() => {
    window.localStorage.setItem(`${STORAGE_PREFIX}.selectedProject`, selectedProject);
  }, [selectedProject]);

  useEffect(() => {
    loadProjects().catch((error) => setStatusMessage(String(error)));
  }, [apiBase]);

  const selectedProjectMeta = useMemo(
    () => projects.find((project) => project.id === selectedProject),
    [projects, selectedProject],
  );

  return {
    projects,
    selectedProject,
    setSelectedProject,
    selectedProjectMeta,
    loadProjects,
  };
}
