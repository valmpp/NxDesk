#!/usr/bin/env python3
"""
run_eval.py — Automated Eval Harness for Antigravity IDE Agent Skills.
Evaluates semantic trigger precision (Should-Trigger vs Should-Not-Trigger).
"""

import os
import sys
import re
import json
import argparse
from pathlib import Path

if hasattr(sys.stdout, 'reconfigure'):
    sys.stdout.reconfigure(encoding='utf-8')

DEFAULT_BATTERY = {
    "arch": {
        "should_trigger": [
            "Diseña la estructura de capas para el módulo de facturación",
            "Crea los contratos de interfaces y DTOs para el bounded context de pedidos",
            "Cómo separar la lógica de dominio de la infraestructura en Clean Architecture",
            "Define the hexagonal architecture ports and adapters for payments",
            "Define las entidades de dominio y value objects para el sistema"
        ],
        "should_not_trigger": [
            "Crea un índice B-Tree en la tabla de usuarios",
            "Diseña un botón con animación hover en CSS",
            "Corrige una vulnerabilidad de XSS en este template",
            "Compila el diagrama de arquitectura con Archify",
            "¿Cuál es el clima en Madrid?"
        ]
    },
    "sec": {
        "should_trigger": [
            "Audita este endpoint contra inyecciones SQL y OWASP Top 10",
            "Revisa si este token JWT valida correctamente issuer y expiration",
            "Cómo sanitizar inputs para prevenir Cross-Site Scripting",
            "Verifica que no haya claves API o secretos hardcodeados",
            "Configura cabeceras CSP y HSTS para producción"
        ],
        "should_not_trigger": [
            "Diseña un componente de tabla con paginación en React",
            "Escribe una migración para agregar una columna a PostgreSQL",
            "Dibuja el diagrama de arquitectura en SVG",
            "Refactoriza la clase de dominio para usar Value Objects",
            "Explica cómo funciona la fotosíntesis"
        ]
    },
    "db-master": {
        "should_trigger": [
            "Crea una migración SQL para agregar índices en PostgreSQL",
            "Optimiza este query con EXPLAIN ANALYZE para eliminar seq scan",
            "Configura PgBouncer en Transaction Mode para escalar conexiones",
            "Crea un índice HNSW para búsqueda vectorial con PgVector",
            "Diseña el esquema relacional para la tabla de inventario"
        ],
        "should_not_trigger": [
            "Crea un componente UI con tema oscuro en Tailwind",
            "Audita el middleware de autenticación JWT",
            "Genera el diagrama interactivo en Archify",
            "Define el contrato DTO en C#",
            "Escribe un script de despliegue en Docker"
        ]
    },
    "ui-expert": {
        "should_trigger": [
            "Crea un componente de tarjeta con diseño glassmorphism y animación",
            "Implementa el selector de tema claro y oscuro en React",
            "Corrige los problemas de contraste y accesibilidad WCAG AA",
            "Diseña un dashboard moderno con micro-interacciones",
            "Optimiza el Cumulative Layout Shift de esta vista web"
        ],
        "should_not_trigger": [
            "Crea una migración de base de datos con claves foráneas",
            "Audita vulnerabilidades de inyección en el backend",
            "Configura el pooling de conexiones con PgBouncer",
            "Define las entidades de dominio en DDD",
            "Calcula el radio de impacto en GitNexus"
        ]
    }
}

def evaluate_skill(skill_dir, battery):
    skill_file = skill_dir / "SKILL.md"
    if not skill_file.exists():
        return None
    
    content = skill_file.read_text(encoding="utf-8")
    desc_match = re.search(r"description:\s*(.*?)(?:\n---|\n[a-z]+:)", content, re.DOTALL)
    description = desc_match.group(1).strip() if desc_match else ""
    
    # Extract keywords from description
    keywords = set(re.findall(r"\b[a-zA-ZáéíóúÁÉÍÓÚñÑ]{4,}\b", description.lower()))
    
    should_trig_results = []
    for query in battery.get("should_trigger", []):
        tokens = set(re.findall(r"\b[a-zA-ZáéíóúÁÉÍÓÚñÑ]{4,}\b", query.lower()))
        matched = bool(tokens & keywords)
        should_trig_results.append({"query": query, "passed": matched})
        
    should_not_results = []
    for query in battery.get("should_not_trigger", []):
        tokens = set(re.findall(r"\b[a-zA-ZáéíóúÁÉÍÓÚñÑ]{4,}\b", query.lower()))
        unwanted_overlap = len(tokens & keywords) >= 3
        should_not_results.append({"query": query, "passed": not unwanted_overlap})
        
    st_passed = sum(1 for r in should_trig_results if r["passed"])
    st_total = len(should_trig_results)
    snt_passed = sum(1 for r in should_not_results if r["passed"])
    snt_total = len(should_not_results)
    
    total_passed = st_passed + snt_passed
    total_cases = st_total + snt_total
    precision = (total_passed / total_cases * 100) if total_cases > 0 else 0
    
    return {
        "skill": skill_dir.name,
        "should_trigger": f"{st_passed}/{st_total}",
        "should_not_trigger": f"{snt_passed}/{snt_total}",
        "precision_pct": round(precision, 1),
        "status": "APPROVED 🟢" if precision >= 90 else "NEEDS_TUNING 🟡",
        "details": {
            "should_trigger": should_trig_results,
            "should_not_trigger": should_not_results
        }
    }

def main():
    parser = argparse.ArgumentParser(description="Run Antigravity IDE Skill Eval Battery")
    parser.add_argument("--agent", help="Specific skill name to evaluate", default=None)
    args = parser.parse_args()

    skills_root = Path(__file__).resolve().parents[2]
    
    results = []
    for target, battery in DEFAULT_BATTERY.items():
        if args.agent and args.agent != target:
            continue
        skill_path = skills_root / target
        if skill_path.exists():
            res = evaluate_skill(skill_path, battery)
            if res:
                results.append(res)
                
    print(json.dumps(results, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()
