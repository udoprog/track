-- Enough cast for the seeded show to pass the page's cap, so the rest open
-- in the cast modal. Named and credited in English (6148257917992593152 and
-- 1701734144, as in seed.sql).
INSERT INTO people (id, department, default_language)
VALUES
    (5101, 'Acting', 6148257917992593152),
    (5102, 'Acting', 6148257917992593152),
    (5103, 'Acting', 6148257917992593152),
    (5104, 'Acting', 6148257917992593152),
    (5105, 'Acting', 6148257917992593152),
    (5106, 'Acting', 6148257917992593152);

INSERT INTO person_strings (person_id, language, kind, text)
VALUES
    (5101, 6148257917992593152, 1, 'Alan Turing'),
    (5102, 6148257917992593152, 1, 'Grace Hopper'),
    (5103, 6148257917992593152, 1, 'Edsger Dijkstra'),
    (5104, 6148257917992593152, 1, 'Barbara Liskov'),
    (5105, 6148257917992593152, 1, 'Donald Knuth'),
    (5106, 6148257917992593152, 1, 'Margaret Hamilton');

INSERT INTO show_credits (id, show_id, person_id, credit_type, department, sort_order)
VALUES
    (6101, 1001, 5101, 0, 'Acting', 2),
    (6102, 1001, 5102, 0, 'Acting', 3),
    (6103, 1001, 5103, 0, 'Acting', 4),
    (6104, 1001, 5104, 0, 'Acting', 5),
    (6105, 1001, 5105, 0, 'Acting', 6),
    (6106, 1001, 5106, 0, 'Acting', 7);

INSERT INTO show_credit_strings (credit_id, language, kind, text)
VALUES
    (6101, 1701734144, 3, 'The Codebreaker'),
    (6102, 1701734144, 3, 'The Admiral'),
    (6103, 1701734144, 3, 'The Pathfinder'),
    (6104, 1701734144, 3, 'The Substitute'),
    (6105, 1701734144, 3, 'The Typesetter'),
    (6106, 1701734144, 3, 'The Flight Director');
