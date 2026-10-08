-- Enough cast for the movie from movie.sql to pass the page's cap, so the
-- rest open in the cast modal.
INSERT INTO people (id, department, default_language)
VALUES
    (5201, 'Acting', 6148257917992593152),
    (5202, 'Acting', 6148257917992593152),
    (5203, 'Acting', 6148257917992593152),
    (5204, 'Acting', 6148257917992593152),
    (5205, 'Acting', 6148257917992593152),
    (5206, 'Acting', 6148257917992593152),
    (5207, 'Acting', 6148257917992593152),
    (5208, 'Acting', 6148257917992593152);

INSERT INTO person_strings (person_id, language, kind, text)
VALUES
    (5201, 6148257917992593152, 1, 'Alan Turing'),
    (5202, 6148257917992593152, 1, 'Grace Hopper'),
    (5203, 6148257917992593152, 1, 'Edsger Dijkstra'),
    (5204, 6148257917992593152, 1, 'Barbara Liskov'),
    (5205, 6148257917992593152, 1, 'Donald Knuth'),
    (5206, 6148257917992593152, 1, 'Margaret Hamilton'),
    (5207, 6148257917992593152, 1, 'Dennis Ritchie'),
    (5208, 6148257917992593152, 1, 'Frances Allen');

INSERT INTO movie_credits (id, movie_id, person_id, credit_type, department, sort_order)
VALUES
    (7201, 4001, 5201, 0, 'Acting', 1),
    (7202, 4001, 5202, 0, 'Acting', 2),
    (7203, 4001, 5203, 0, 'Acting', 3),
    (7204, 4001, 5204, 0, 'Acting', 4),
    (7205, 4001, 5205, 0, 'Acting', 5),
    (7206, 4001, 5206, 0, 'Acting', 6),
    (7207, 4001, 5207, 0, 'Acting', 7),
    (7208, 4001, 5208, 0, 'Acting', 8);

INSERT INTO movie_credit_strings (credit_id, language, kind, text)
VALUES
    (7201, 1701734144, 3, 'The Codebreaker'),
    (7202, 1701734144, 3, 'The Admiral'),
    (7203, 1701734144, 3, 'The Pathfinder'),
    (7204, 1701734144, 3, 'The Substitute'),
    (7205, 1701734144, 3, 'The Typesetter'),
    (7206, 1701734144, 3, 'The Flight Director'),
    (7207, 1701734144, 3, 'The Toolmaker'),
    (7208, 1701734144, 3, 'The Compiler');
